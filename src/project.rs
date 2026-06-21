use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::checker::{Type, check_source_in_package};
use crate::error::VerseError;
use crate::parser::parse_source;
use crate::pipeline::run_source_in_package;
use crate::runtime::Value;
use crate::token::Span;

pub fn load_project_source(path: impl AsRef<Path>) -> Result<String, VerseError> {
    SourceProject::from_path(path.as_ref())?.load_source()
}

pub fn check_project_file(path: impl AsRef<Path>) -> Result<Type, VerseError> {
    let project = SourceProject::from_path(path.as_ref())?;
    let source = project.load_source()?;
    check_source_in_package(&source, project.package.as_deref())
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
        })
    }

    pub fn load_source(&self) -> Result<String, VerseError> {
        ProjectLoader::new(self.clone()).load()
    }
}

struct ProjectManifest {
    entry: Option<PathBuf>,
    package: Option<String>,
}

struct ProjectLoader {
    root: PathBuf,
    entry: PathBuf,
    loaded: HashSet<PathBuf>,
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
            _ => {
                return Err(VerseError::parse(
                    format!("unknown project manifest key `{key}` in {}", path.display()),
                    Span::new(0, 0, index + 1, 1),
                ));
            }
        }
    }

    Ok(manifest)
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

fn absolute_from(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

impl ProjectLoader {
    fn new(project: SourceProject) -> Self {
        Self {
            root: project.root,
            entry: project.entry,
            loaded: HashSet::new(),
            sources: Vec::new(),
        }
    }

    fn load(mut self) -> Result<String, VerseError> {
        let entry_source = read_source_file(&self.entry)?;
        let entry_program = parse_source(&entry_source)?;
        let imports = collect_local_imports(&entry_program, &[]);
        for import in imports {
            self.load_import(&import)?;
        }
        self.load_implicit_root_modules()?;
        self.load_implicit_root_sources()?;
        self.sources
            .push(render_source_chunk(&self.entry, &entry_source));
        Ok(self.sources.join("\n"))
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
