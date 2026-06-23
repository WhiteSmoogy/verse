use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{ExprKind, Program, Stmt, StmtKind, TypeName};
use crate::checker::{Type, check_source_in_package};
use crate::digest::generate_digest_for_program;
use crate::error::VerseError;
use crate::parser::parse_source;
use crate::pipeline::run_source_in_package;
use crate::runtime::Value;
use crate::token::Span;

pub fn load_project_source(path: impl AsRef<Path>) -> Result<String, VerseError> {
    SourceProject::from_path(path.as_ref())?.load_source()
}

pub(crate) fn load_project_own_source(path: impl AsRef<Path>) -> Result<String, VerseError> {
    let project = SourceProject::from_path(path.as_ref())?;
    ProjectLoader::new(project.without_dependencies()).load_own_source()
}

pub fn check_project_file(path: impl AsRef<Path>) -> Result<Type, VerseError> {
    let project = SourceProject::from_path(path.as_ref())?;
    let loaded = project.load_with_constraints()?;
    let value_type = check_source_in_package(&loaded.source, project.package.as_deref())?;
    check_persistence_constraints(
        &loaded.source,
        project.package.as_deref(),
        &loaded.persistence_constraints,
        &project.persistence_scope_remaps,
    )?;
    Ok(value_type)
}

pub fn run_project_file(path: impl AsRef<Path>) -> Result<Value, VerseError> {
    let project = SourceProject::from_path(path.as_ref())?;
    let source = project.load_source()?;
    run_source_in_package(&source, project.package.as_deref())
}

#[derive(Debug, Clone)]
pub struct SourceProject {
    pub root: PathBuf,
    pub entry: PathBuf,
    pub package: Option<String>,
    pub role: PackageRole,
    pub verse_version: Option<u32>,
    pub uploaded_at_fn_version: Option<u32>,
    pub persistence_scope_remaps: Vec<PersistenceScopeRemap>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageRole {
    Source,
    External,
    GeneralCompatConstraint,
    PersistenceCompatConstraint,
    PersistenceSoftCompatConstraint,
}

impl PackageRole {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "Source" => Some(Self::Source),
            "External" => Some(Self::External),
            "GeneralCompatConstraint" => Some(Self::GeneralCompatConstraint),
            "PersistenceCompatConstraint" => Some(Self::PersistenceCompatConstraint),
            "PersistenceSoftCompatConstraint" => Some(Self::PersistenceSoftCompatConstraint),
            _ => None,
        }
    }

    fn is_persistence_constraint(self) -> bool {
        matches!(
            self,
            Self::PersistenceCompatConstraint | Self::PersistenceSoftCompatConstraint
        )
    }

    fn is_soft_persistence_constraint(self) -> bool {
        matches!(self, Self::PersistenceSoftCompatConstraint)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceScopeRemap {
    pub from: String,
    pub to: String,
}

impl SourceProject {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, VerseError> {
        let path = path.as_ref();
        if path
            .extension()
            .is_some_and(|extension| extension == "vproject")
        {
            return Self::from_manifest(path);
        }

        if let Some(manifest) = find_manifest_for_entry(path)? {
            let mut project = Self::from_manifest(&manifest)?;
            project.entry = absolute_from(&project.root, path);
            return Ok(project);
        }

        let entry = path.to_path_buf();
        let root = entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(Self {
            root,
            entry,
            package: None,
            role: PackageRole::Source,
            verse_version: None,
            uploaded_at_fn_version: None,
            persistence_scope_remaps: Vec::new(),
            dependencies: Vec::new(),
        })
    }

    pub fn from_manifest(path: impl AsRef<Path>) -> Result<Self, VerseError> {
        let path = path.as_ref();
        let root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let source = read_source_file(path)?;
        let manifest = parse_project_manifest(&source, path)?;
        let entry = manifest
            .entry
            .unwrap_or_else(|| PathBuf::from("main.verse"));
        Ok(Self {
            entry: absolute_from(&root, &entry),
            root,
            package: manifest.package,
            role: manifest.role,
            verse_version: manifest.verse_version,
            uploaded_at_fn_version: manifest.uploaded_at_fn_version,
            persistence_scope_remaps: manifest.persistence_scope_remaps,
            dependencies: manifest.dependencies,
        })
    }

    pub fn load_source(&self) -> Result<String, VerseError> {
        ProjectLoader::new(self.clone())
            .load()
            .map(|loaded| loaded.source)
    }

    fn load_with_constraints(&self) -> Result<LoadedProject, VerseError> {
        ProjectLoader::new(self.clone()).load()
    }

    fn without_dependencies(&self) -> Self {
        let mut project = self.clone();
        project.dependencies.clear();
        project
    }
}

struct ProjectManifest {
    entry: Option<PathBuf>,
    package: Option<String>,
    role: PackageRole,
    verse_version: Option<u32>,
    uploaded_at_fn_version: Option<u32>,
    persistence_scope_remaps: Vec<PersistenceScopeRemap>,
    dependencies: Vec<String>,
}

struct LoadedProject {
    source: String,
    persistence_constraints: Vec<PersistenceConstraintPackage>,
}

struct PersistenceConstraintPackage {
    package: String,
    source: String,
    soft: bool,
    scope_remaps: Vec<PersistenceScopeRemap>,
}

struct ProjectLoader {
    root: PathBuf,
    entry: PathBuf,
    package: Option<String>,
    dependencies: Vec<String>,
    direct_dependency_packages: HashSet<String>,
    dependency_module_packages: HashMap<String, String>,
    loaded: HashSet<PathBuf>,
    loaded_dependency_digests: HashSet<PathBuf>,
    persistence_constraints: Vec<PersistenceConstraintPackage>,
    sources: Vec<String>,
}

struct ModuleText {
    key: PathBuf,
    also_loaded: Vec<PathBuf>,
    source: String,
    imports: Vec<String>,
}

fn parse_project_manifest(source: &str, path: &Path) -> Result<ProjectManifest, VerseError> {
    let mut manifest = ProjectManifest {
        entry: None,
        package: None,
        role: PackageRole::Source,
        verse_version: None,
        uploaded_at_fn_version: None,
        persistence_scope_remaps: Vec::new(),
        dependencies: Vec::new(),
    };

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=').or_else(|| trimmed.split_once(':')) else {
            return Err(VerseError::parse(
                format!(
                    "invalid project manifest line {} in {}",
                    index + 1,
                    path.display()
                ),
                Span::new(0, 0, index + 1, 1),
            ));
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match key {
            "entry" => manifest.entry = Some(PathBuf::from(value)),
            "package" => manifest.package = Some(value.to_string()),
            "role" => {
                manifest.role = PackageRole::parse(value).ok_or_else(|| {
                    VerseError::parse(
                        format!("unknown package role `{value}` in {}", path.display()),
                        Span::new(0, 0, index + 1, 1),
                    )
                })?;
            }
            "verseVersion" => {
                let version = parse_manifest_u32(value, key, path, index + 1)?;
                if version > 2 {
                    return Err(VerseError::parse(
                        format!(
                            "unsupported Verse version `{version}` in {}",
                            path.display()
                        ),
                        Span::new(0, 0, index + 1, 1),
                    ));
                }
                manifest.verse_version = Some(version);
            }
            "uploadedAtFNVersion" => {
                manifest.uploaded_at_fn_version =
                    Some(parse_manifest_u32(value, key, path, index + 1)?);
            }
            "persistenceScopeRemap" | "persistenceScopeRemaps" => {
                manifest
                    .persistence_scope_remaps
                    .extend(parse_persistence_scope_remaps(value));
            }
            "dependencyPackages" | "dependencies" | "dependency" => {
                manifest.dependencies.extend(parse_dependency_list(value));
            }
            _ => {
                return Err(VerseError::parse(
                    format!("unknown project manifest key `{key}` in {}", path.display()),
                    Span::new(0, 0, index + 1, 1),
                ));
            }
        }
    }

    validate_manifest_dependencies(&manifest, path)?;
    Ok(manifest)
}

fn parse_manifest_u32(value: &str, key: &str, path: &Path, line: usize) -> Result<u32, VerseError> {
    value.parse::<u32>().map_err(|_| {
        VerseError::parse(
            format!(
                "manifest key `{key}` expected unsigned integer in {}",
                path.display()
            ),
            Span::new(0, 0, line, 1),
        )
    })
}

fn parse_dependency_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|dependency| dependency.trim().trim_matches('"').trim_matches('\''))
        .filter(|dependency| !dependency.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_persistence_scope_remaps(value: &str) -> Vec<PersistenceScopeRemap> {
    parse_dependency_list(value)
        .into_iter()
        .filter_map(|entry| {
            let (from, to) = entry
                .split_once("=>")
                .or_else(|| entry.split_once("->"))
                .or_else(|| entry.split_once(':'))?;
            Some(PersistenceScopeRemap {
                from: from.trim().to_string(),
                to: to.trim().to_string(),
            })
        })
        .filter(|remap| !remap.from.is_empty() && !remap.to.is_empty())
        .collect()
}

fn validate_manifest_dependencies(
    manifest: &ProjectManifest,
    path: &Path,
) -> Result<(), VerseError> {
    let mut seen = HashSet::new();
    for dependency in &manifest.dependencies {
        if manifest.package.as_ref() == Some(dependency) {
            return Err(VerseError::parse(
                format!(
                    "package `{dependency}` cannot depend on itself in {}",
                    path.display()
                ),
                Span::new(0, 0, 1, 1),
            ));
        }
        if !seen.insert(dependency) {
            return Err(VerseError::parse(
                format!(
                    "duplicate dependency package `{dependency}` in {}",
                    path.display()
                ),
                Span::new(0, 0, 1, 1),
            ));
        }
    }
    Ok(())
}

fn check_persistence_constraints(
    source: &str,
    package: Option<&str>,
    constraints: &[PersistenceConstraintPackage],
    scope_remaps: &[PersistenceScopeRemap],
) -> Result<(), VerseError> {
    let current = extract_persistence_schema(source)?;
    for constraint in constraints {
        let expected = extract_persistence_schema(&constraint.source)?;
        let mut remaps = constraint.scope_remaps.clone();
        remaps.extend(scope_remaps.iter().cloned());
        for (path, expected_type) in expected.weak_maps {
            let current_path = remap_persistence_path(&path, &constraint.package, package, &remaps);
            let Some(actual_type) = current.weak_maps.get(&current_path) else {
                if constraint.soft {
                    continue;
                }
                return Err(VerseError::parse(
                    format!(
                        "persistent weak_map `{path}` is missing in current package for persistence constraint package `{}`",
                        constraint.package
                    ),
                    Span::new(0, 0, 1, 1),
                ));
            };
            if actual_type != &expected_type {
                if constraint.soft {
                    continue;
                }
                return Err(VerseError::parse(
                    format!(
                        "persistent weak_map `{path}` is not backward-compatible with persistence constraint package `{}`",
                        constraint.package
                    ),
                    Span::new(0, 0, 1, 1),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct PersistenceSchema {
    weak_maps: HashMap<String, (TypeName, TypeName)>,
}

fn extract_persistence_schema(source: &str) -> Result<PersistenceSchema, VerseError> {
    let program = parse_source(source)?;
    let mut schema = PersistenceSchema::default();
    collect_persistence_schema(&program.statements, &mut Vec::new(), &mut schema);
    Ok(schema)
}

fn collect_persistence_schema(
    statements: &[Stmt],
    module_path: &mut Vec<String>,
    schema: &mut PersistenceSchema,
) {
    for statement in statements {
        match &statement.kind {
            StmtKind::Let { name, expr, .. } => {
                if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
                    module_path.push(name.clone());
                    collect_persistence_schema(statements, module_path, schema);
                    module_path.pop();
                }
            }
            StmtKind::Var {
                name,
                annotation: Some(annotation),
                ..
            } => {
                if let TypeName::WeakMap(key, value) = &annotation.name {
                    let path = qualified_schema_path(module_path, name);
                    schema
                        .weak_maps
                        .insert(path, ((**key).clone(), (**value).clone()));
                }
            }
            _ => {}
        }
    }
}

fn qualified_schema_path(module_path: &[String], name: &str) -> String {
    if module_path.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", module_path.join("."), name)
    }
}

fn remap_persistence_path(
    path: &str,
    constraint_package: &str,
    current_package: Option<&str>,
    remaps: &[PersistenceScopeRemap],
) -> String {
    for remap in remaps {
        let from = strip_package_prefix(&remap.from, Some(constraint_package));
        let to = strip_package_prefix(&remap.to, current_package);
        if path == from {
            return to;
        }
        if let Some(suffix) = path.strip_prefix(&format!("{from}.")) {
            return format!("{to}.{suffix}");
        }
    }
    path.to_string()
}

fn strip_package_prefix(path: &str, package: Option<&str>) -> String {
    let Some(package) = package else {
        return path.to_string();
    };
    if path == package {
        String::new()
    } else if let Some(stripped) = path.strip_prefix(&format!("{package}.")) {
        stripped.to_string()
    } else {
        path.to_string()
    }
}

fn find_manifest_for_entry(entry: &Path) -> Result<Option<PathBuf>, VerseError> {
    let mut dir = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    loop {
        let manifests = read_dir_entries(&dir)?
            .into_iter()
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "vproject")
            })
            .collect::<Vec<_>>();
        if manifests.len() > 1 {
            return Err(VerseError::parse(
                format!("multiple `.vproject` manifests found in {}", dir.display()),
                Span::new(0, 0, 1, 1),
            ));
        }
        if let Some(manifest) = manifests.into_iter().next() {
            return Ok(Some(manifest));
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

fn find_dependency_manifest(
    root: &Path,
    package: Option<&str>,
    dependency: &str,
) -> Result<PathBuf, VerseError> {
    let mut matches = Vec::new();
    for search_root in dependency_search_roots(root, package) {
        for manifest in package_manifest_candidates(&search_root)? {
            let source = read_source_file(&manifest)?;
            let candidate = parse_project_manifest(&source, &manifest)?;
            if candidate.package.as_deref() == Some(dependency) {
                matches.push(manifest);
            }
        }
    }
    matches.sort();
    matches.dedup();

    match matches.len() {
        0 => Err(VerseError::parse(
            format!(
                "unknown dependency package `{dependency}` near {}",
                root.display()
            ),
            Span::new(0, 0, 1, 1),
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(VerseError::parse(
            format!(
                "multiple `.vproject` manifests define dependency package `{dependency}` near {}",
                root.display()
            ),
            Span::new(0, 0, 1, 1),
        )),
    }
}

fn dependency_search_roots(root: &Path, package: Option<&str>) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| Some(name) == package)
        && let Some(parent) = root.parent()
    {
        roots.push(parent.to_path_buf());
    }
    roots
}

fn package_manifest_candidates(search_root: &Path) -> Result<Vec<PathBuf>, VerseError> {
    let mut manifests = Vec::new();
    manifests.extend(manifest_files_in(search_root)?);
    for dir in child_dirs(search_root)? {
        manifests.extend(manifest_files_in(&dir)?);
    }
    manifests.sort();
    Ok(manifests)
}

fn manifest_files_in(dir: &Path) -> Result<Vec<PathBuf>, VerseError> {
    read_dir_entries(dir).map(|entries| {
        entries
            .into_iter()
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "vproject")
            })
            .collect()
    })
}

fn absolute_from(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

impl ProjectLoader {
    fn new(project: SourceProject) -> Self {
        let dependencies = project.dependencies;
        let direct_dependency_packages = dependencies.iter().cloned().collect();
        Self {
            root: project.root,
            entry: project.entry,
            package: project.package,
            dependencies,
            direct_dependency_packages,
            dependency_module_packages: HashMap::new(),
            loaded: HashSet::new(),
            loaded_dependency_digests: HashSet::new(),
            persistence_constraints: Vec::new(),
            sources: Vec::new(),
        }
    }

    fn load(mut self) -> Result<LoadedProject, VerseError> {
        self.load_dependency_digests()?;
        self.load_own_sources()?;
        Ok(LoadedProject {
            source: self.sources.join("\n"),
            persistence_constraints: self.persistence_constraints,
        })
    }

    fn load_own_source(mut self) -> Result<String, VerseError> {
        self.load_own_sources()?;
        Ok(self.sources.join("\n"))
    }

    fn load_own_sources(&mut self) -> Result<(), VerseError> {
        let entry_source = read_source_file(&self.entry)?;
        let entry_program = parse_source(&entry_source)?;
        let imports = collect_local_imports(&entry_program, &[]);
        for import in imports {
            self.ensure_declared_dependency_import(&import)?;
            self.load_import(&import)?;
        }
        self.load_implicit_root_modules()?;
        self.load_implicit_root_sources()?;
        self.sources
            .push(render_source_chunk(&self.entry, &entry_source));
        Ok(())
    }

    fn load_dependency_digests(&mut self) -> Result<(), VerseError> {
        let dependencies = self.dependencies.clone();
        let root = self.root.clone();
        let package = self.package.clone();
        let mut visiting = HashSet::new();
        for dependency in dependencies {
            self.load_dependency_digest_recursive(
                &root,
                package.as_deref(),
                &dependency,
                &mut visiting,
            )?;
        }
        Ok(())
    }

    fn load_dependency_digest_recursive(
        &mut self,
        root: &Path,
        package: Option<&str>,
        dependency: &str,
        visiting: &mut HashSet<PathBuf>,
    ) -> Result<(), VerseError> {
        let manifest = find_dependency_manifest(root, package, dependency)?;
        let key = canonical_key(&manifest);
        if self.loaded_dependency_digests.contains(&key) || !visiting.insert(key.clone()) {
            return Ok(());
        }

        let project = SourceProject::from_manifest(&manifest)?;
        let role = project.role;
        let dependencies = project.dependencies.clone();
        let dependency_root = project.root.clone();
        let dependency_package = project.package.clone();
        for dependency in dependencies {
            self.load_dependency_digest_recursive(
                &dependency_root,
                dependency_package.as_deref(),
                &dependency,
                visiting,
            )?;
        }

        let source = ProjectLoader::new(project.without_dependencies()).load_own_source()?;
        if role.is_persistence_constraint() {
            self.persistence_constraints
                .push(PersistenceConstraintPackage {
                    package: dependency.to_string(),
                    source,
                    soft: role.is_soft_persistence_constraint(),
                    scope_remaps: project.persistence_scope_remaps,
                });
            visiting.remove(&key);
            self.loaded_dependency_digests.insert(key);
            return Ok(());
        }

        let program = parse_source(&source)?;
        let digest = generate_digest_for_program(&program);
        if !digest.trim().is_empty() {
            let digest_program = parse_source(&digest)?;
            self.record_dependency_digest_modules(&digest_program, project.package.as_deref());
            self.sources
                .push(render_source_chunk(&manifest, digest.trim()));
        }
        visiting.remove(&key);
        self.loaded_dependency_digests.insert(key);
        Ok(())
    }

    fn record_dependency_digest_modules(&mut self, program: &Program, package: Option<&str>) {
        let Some(package) = package else {
            return;
        };
        let package = package.to_string();
        let package_is_direct = self.direct_dependency_packages.contains(&package);
        for statement in &program.statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            if !matches!(expr.kind, ExprKind::ModuleDefinition { .. }) {
                continue;
            }
            let existing_is_direct = self
                .dependency_module_packages
                .get(name)
                .is_some_and(|existing| self.direct_dependency_packages.contains(existing));
            if package_is_direct || !existing_is_direct {
                self.dependency_module_packages
                    .insert(name.clone(), package.clone());
            }
        }
    }

    fn ensure_declared_dependency_import(&self, module_path: &str) -> Result<(), VerseError> {
        if self.local_module_may_exist(module_path) {
            return Ok(());
        }
        let root = module_path.split('.').next().unwrap_or(module_path);
        let Some(package) = self.dependency_module_packages.get(root) else {
            return Ok(());
        };
        if self.direct_dependency_packages.contains(package) {
            return Ok(());
        }
        let current_package = self.package.as_deref().unwrap_or("<unknown>");
        Err(VerseError::parse(
            format!(
                "module `{module_path}` is defined in dependency package `{package}`, but package `{current_package}` does not declare a direct dependency on `{package}`"
            ),
            Span::new(0, 0, 1, 1),
        ))
    }

    fn local_module_may_exist(&self, module_path: &str) -> bool {
        let parts = module_path
            .split('.')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let Some(first) = parts.first() else {
            return false;
        };
        if self.root.join(format!("{first}.verse")).is_file() || self.root.join(first).is_dir() {
            return true;
        }
        if parts.len() > 1 {
            let full = self.root.join(parts.iter().collect::<PathBuf>());
            full.with_extension("verse").is_file() || full.is_dir()
        } else {
            false
        }
    }

    fn load_import(&mut self, module_path: &str) -> Result<(), VerseError> {
        let Some(module_text) = self.resolve_module(module_path)? else {
            return Ok(());
        };

        self.load_module_text(module_text)
    }

    fn load_module_text(&mut self, module_text: ModuleText) -> Result<(), VerseError> {
        if !self.loaded.insert(module_text.key.clone()) {
            return Ok(());
        }
        for key in &module_text.also_loaded {
            self.loaded.insert(key.clone());
        }

        for import in &module_text.imports {
            self.ensure_declared_dependency_import(import)?;
            self.load_import(import)?;
        }

        self.sources
            .push(render_source_chunk(&module_text.key, &module_text.source));
        Ok(())
    }

    fn load_implicit_root_modules(&mut self) -> Result<(), VerseError> {
        let entry_key = canonical_key(&self.entry);

        for file in verse_files(&self.root)? {
            let key = canonical_key(&file);
            if key == entry_key {
                continue;
            }
            let Some(module_text) = explicit_file_module_text(&file)? else {
                continue;
            };
            self.load_module_text(module_text)?;
        }

        for dir in child_dirs(&self.root)? {
            if self.loaded.contains(&canonical_key(&dir)) {
                continue;
            }
            if verse_files(&dir)?.is_empty() {
                continue;
            }
            let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let module_parts = vec![name.to_string()];
            let (body, imports) = read_directory_module_body(&dir, &module_parts)?;
            self.load_module_text(ModuleText {
                key: canonical_key(&dir),
                also_loaded: Vec::new(),
                imports,
                source: wrap_module_path(&module_parts, &body),
            })?;
        }

        Ok(())
    }

    fn load_implicit_root_sources(&mut self) -> Result<(), VerseError> {
        let entry_key = canonical_key(&self.entry);

        for file in verse_files(&self.root)? {
            let key = canonical_key(&file);
            if key == entry_key || self.loaded.contains(&key) {
                continue;
            }
            let Some(module_text) = ordinary_file_declarations_text(&file)? else {
                continue;
            };
            self.load_module_text(module_text)?;
        }

        Ok(())
    }

    fn resolve_module(&self, module_path: &str) -> Result<Option<ModuleText>, VerseError> {
        let parts = module_path
            .split('.')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Ok(None);
        }

        let first_file = self.root.join(format!("{}.verse", parts[0]));
        if first_file.is_file() {
            let first_dir = self.root.join(parts[0]);
            if first_dir.is_dir() {
                return explicit_file_directory_module_text(
                    &first_file,
                    &first_dir,
                    &[parts[0].to_string()],
                    &[],
                );
            }
            let source = read_source_file(&first_file)?;
            let program = parse_source(&source)?;
            return Ok(Some(ModuleText {
                key: canonical_key(&first_file),
                also_loaded: Vec::new(),
                imports: collect_local_imports(&program, &[]),
                source,
            }));
        }

        let full_file = self
            .root
            .join(parts.iter().collect::<PathBuf>())
            .with_extension("verse");
        if full_file.is_file() {
            let full_dir = self.root.join(parts.iter().collect::<PathBuf>());
            let module_parts = parts
                .iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>();
            let source = read_source_file(&full_file)?;
            let context = parts[..parts.len().saturating_sub(1)]
                .iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>();
            if full_dir.is_dir() {
                return explicit_file_directory_module_text(
                    &full_file,
                    &full_dir,
                    &module_parts,
                    &context,
                );
            }
            let program = parse_source(&source)?;
            let source = if context.is_empty() {
                source
            } else {
                wrap_module_path(&context, &source)
            };
            return Ok(Some(ModuleText {
                key: canonical_key(&full_file),
                also_loaded: Vec::new(),
                imports: collect_local_imports(&program, &context),
                source,
            }));
        }

        let full_dir = self.root.join(parts.iter().collect::<PathBuf>());
        if full_dir.is_dir() {
            let module_parts = parts
                .iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>();
            let (body, imports) = read_directory_module_body(&full_dir, &module_parts)?;
            return Ok(Some(ModuleText {
                key: canonical_key(&full_dir),
                also_loaded: Vec::new(),
                imports,
                source: wrap_module_path(&module_parts, &body),
            }));
        }

        Ok(None)
    }
}

fn ordinary_file_declarations_text(path: &Path) -> Result<Option<ModuleText>, VerseError> {
    let source = read_source_file(path)?;
    let program = parse_source(&source)?;
    if program
        .statements
        .iter()
        .any(|statement| !is_module_level_declaration(statement))
    {
        return Ok(None);
    }
    let declaration_source = declaration_only_source(&source, &program);
    if declaration_source.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(ModuleText {
        key: canonical_key(path),
        also_loaded: Vec::new(),
        imports: collect_local_imports(&program, &[]),
        source: declaration_source,
    }))
}

fn explicit_file_module_text(path: &Path) -> Result<Option<ModuleText>, VerseError> {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(None);
    };
    let sibling_dir = path.with_extension("");
    if sibling_dir.is_dir() {
        return explicit_file_directory_module_text(path, &sibling_dir, &[stem.to_string()], &[]);
    }
    let source = read_source_file(path)?;
    if !source_may_define_explicit_module(stem, &source) {
        return Ok(None);
    }
    let program = parse_source(&source)?;
    let has_matching_module = program.statements.iter().any(|statement| {
        matches!(
            &statement.kind,
            StmtKind::Let {
                name,
                expr,
                ..
            } if name == stem && matches!(expr.kind, ExprKind::ModuleDefinition { .. })
        )
    });
    if !has_matching_module {
        return Ok(None);
    }

    Ok(Some(ModuleText {
        key: canonical_key(path),
        also_loaded: Vec::new(),
        imports: collect_local_imports(&program, &[]),
        source,
    }))
}

fn explicit_file_directory_module_text(
    file: &Path,
    dir: &Path,
    module_parts: &[String],
    context: &[String],
) -> Result<Option<ModuleText>, VerseError> {
    let Some(module_name) = module_parts.last() else {
        return Ok(None);
    };
    let source = read_source_file(file)?;
    if !source_may_define_explicit_module(module_name, &source) {
        return Ok(None);
    }
    let program = parse_source(&source)?;
    let Some((specifiers, statements)) = explicit_module_definition_parts(&program, module_name)
    else {
        return Ok(None);
    };

    let descriptor_body = module_body_source(&source, statements);
    let (directory_body, directory_imports) = read_directory_module_body(dir, module_parts)?;
    let mut body_parts = Vec::new();
    if !descriptor_body.trim().is_empty() {
        body_parts.push(descriptor_body);
    }
    if !directory_body.trim().is_empty() {
        body_parts.push(directory_body);
    }
    let body = body_parts.join("\n");
    let source = wrap_module_path_with_leaf_specifiers(module_parts, specifiers, &body);

    let mut imports = collect_local_imports(&program, context);
    imports.extend(directory_imports);

    Ok(Some(ModuleText {
        key: canonical_key(file),
        also_loaded: vec![canonical_key(dir)],
        imports,
        source,
    }))
}

fn explicit_module_definition_parts<'a>(
    program: &'a Program,
    module_name: &str,
) -> Option<(&'a [String], &'a [Stmt])> {
    program.statements.iter().find_map(|statement| {
        let StmtKind::Let {
            name,
            specifiers,
            expr,
            ..
        } = &statement.kind
        else {
            return None;
        };
        let ExprKind::ModuleDefinition { statements, .. } = &expr.kind else {
            return None;
        };
        (name == module_name).then_some((specifiers.as_slice(), statements.as_slice()))
    })
}

fn source_may_define_explicit_module(stem: &str, source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(stem) else {
            return false;
        };
        let compact = rest
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        (compact.starts_with(":=module") || compact.starts_with('<'))
            && compact.contains(":=module")
    })
}

fn declaration_only_source(source: &str, program: &Program) -> String {
    program
        .statements
        .iter()
        .filter(|statement| is_module_level_declaration(statement))
        .filter_map(|statement| source.get(statement.span.start..statement.span.end))
        .collect::<Vec<_>>()
        .join("\n")
}

fn module_body_source(source: &str, statements: &[Stmt]) -> String {
    statements
        .iter()
        .filter_map(|statement| source.get(statement.span.start..statement.span.end))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_module_level_declaration(statement: &Stmt) -> bool {
    matches!(
        statement.kind,
        StmtKind::Using { .. }
            | StmtKind::Let { .. }
            | StmtKind::ParametricType { .. }
            | StmtKind::ParametricTypeAlias { .. }
            | StmtKind::TypeAlias { .. }
            | StmtKind::ExtensionMethod(_)
            | StmtKind::Var { .. }
    )
}

fn read_directory_module_body(
    dir: &Path,
    module_path: &[String],
) -> Result<(String, Vec<String>), VerseError> {
    let mut body = String::new();
    let mut imports = Vec::new();

    for file in verse_files(dir)? {
        let source = read_source_file(&file)?;
        let program = parse_source(&source)?;
        imports.extend(collect_local_imports(&program, module_path));
        body.push_str(&render_source_chunk(&file, &source));
        body.push('\n');
    }

    for subdir in child_dirs(dir)? {
        let Some(name) = subdir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let mut child_module_path = module_path.to_vec();
        child_module_path.push(name.to_string());
        let (child_body, child_imports) = read_directory_module_body(&subdir, &child_module_path)?;
        imports.extend(child_imports);
        body.push_str(&format!("{name} := module:\n"));
        body.push_str(&indent_source(&child_body));
        body.push('\n');
    }

    if body.trim().is_empty() {
        body.push_str("false\n");
    }

    Ok((body, imports))
}

fn collect_local_imports(program: &Program, module_path: &[String]) -> Vec<String> {
    let mut imports = Vec::new();
    collect_local_imports_from_statements(&program.statements, module_path, &mut imports);
    imports
}

fn collect_local_imports_from_statements(
    statements: &[Stmt],
    module_path: &[String],
    imports: &mut Vec<String>,
) {
    for statement in statements {
        match &statement.kind {
            StmtKind::Using { path } if !path.starts_with('/') => {
                if !path.contains('.') && !module_path.is_empty() {
                    imports.push(format!("{}.{}", module_path.join("."), path));
                }
                imports.push(path.clone());
            }
            StmtKind::Let { name, expr, .. } => {
                if let ExprKind::ModuleDefinition { statements, .. } = &expr.kind {
                    let mut child_path = module_path.to_vec();
                    child_path.push(name.clone());
                    collect_local_imports_from_statements(statements, &child_path, imports);
                }
            }
            _ => {}
        }
    }
}

fn wrap_module_path(parts: &[String], body: &str) -> String {
    let mut source = body.to_string();
    for part in parts.iter().rev() {
        source = format!("{part} := module:\n{}", indent_source(&source));
    }
    source
}

fn wrap_module_path_with_leaf_specifiers(
    parts: &[String],
    leaf_specifiers: &[String],
    body: &str,
) -> String {
    let Some((leaf, parents)) = parts.split_last() else {
        return body.to_string();
    };
    let mut source = render_module_definition(leaf, leaf_specifiers, body);
    for part in parents.iter().rev() {
        source = render_module_definition(part, &[], &source);
    }
    source
}

fn render_module_definition(name: &str, specifiers: &[String], body: &str) -> String {
    let specifiers = specifiers
        .iter()
        .map(|specifier| format!("<{specifier}>"))
        .collect::<String>();
    if body.trim().is_empty() {
        format!("{name}{specifiers} := module {{}}")
    } else {
        format!("{name}{specifiers} := module:\n{}", indent_source(body))
    }
}

fn indent_source(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn render_source_chunk(path: &Path, source: &str) -> String {
    format!("# from {}\n{}\n", path.display(), source.trim())
}

fn read_source_file(path: &Path) -> Result<String, VerseError> {
    fs::read_to_string(path).map_err(|error| {
        VerseError::parse(
            format!("failed to read {}: {error}", path.display()),
            Span::new(0, 0, 1, 1),
        )
    })
}

fn verse_files(dir: &Path) -> Result<Vec<PathBuf>, VerseError> {
    let mut files = read_dir_entries(dir)?
        .into_iter()
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "verse")
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(files)
}

fn child_dirs(dir: &Path) -> Result<Vec<PathBuf>, VerseError> {
    let mut dirs = read_dir_entries(dir)?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(dirs)
}

fn read_dir_entries(dir: &Path) -> Result<Vec<PathBuf>, VerseError> {
    fs::read_dir(dir)
        .map_err(|error| {
            VerseError::parse(
                format!("failed to read {}: {error}", dir.display()),
                Span::new(0, 0, 1, 1),
            )
        })?
        .map(|entry| {
            entry.map(|entry| entry.path()).map_err(|error| {
                VerseError::parse(
                    format!("failed to read {}: {error}", dir.display()),
                    Span::new(0, 0, 1, 1),
                )
            })
        })
        .collect()
}

fn canonical_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
