//! Executable inventory for finishing the remaining Persistence/evolution FT.
//! Ignored tests are planned work columns; unignore one column, make it pass, then commit.

mod common;
use common::*;

fn write_profile_project(root: &std::path::Path, package: &str, role: Option<&str>, source: &str) {
    let role = role
        .map(|role| format!("role = {role}\nverseVersion = 1\nuploadedAtFNVersion = 3430\n"))
        .unwrap_or_default();
    write_project_file(
        root,
        &format!("{package}\\{package}.vproject"),
        &format!("package = {package}\n{role}entry = main.verse\n"),
    );
    write_project_file(root, &format!("{package}\\main.verse"), source);
}

fn assert_project_check_error(path: std::path::PathBuf, expected: &str) {
    let error = check_project_file(path).expect_err("project should fail");
    assert!(
        error.to_string().contains(expected),
        "expected error containing `{expected}`, got {error}"
    );
}

#[test]
fn checks_persistence_evolution_column_constraint_package_loading_and_versions() {
    let root = temp_project_dir("persistence_evolution_constraints");
    write_profile_project(
        &root,
        "PublishedProfile",
        Some("PersistenceCompatConstraint"),
        r#"
Profile<public> := module:
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
    var Saved:weak_map(player, profile_data) = map{}
"#,
    );
    write_profile_project(
        &root,
        "Game",
        None,
        r#"
Profile<public> := module:
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
        Level<public>:int = 1
    var Saved:weak_map(player, profile_data) = map{}
Profile.profile_data{XP := 40, Level := 2}.XP + 2
"#,
    );
    write_project_file(
        &root,
        "Game\\Game.vproject",
        "package = Game\nentry = main.verse\ndependencyPackages = PublishedProfile\n",
    );

    assert_eq!(
        check_project_file(root.join("Game\\Game.vproject")).expect("project should check"),
        Type::Int
    );

    write_project_file(
        &root,
        "BadRole\\BadRole.vproject",
        "package = BadRole\nentry = main.verse\nrole = DefinitelyNotARole\n",
    );
    write_project_file(&root, "BadRole\\main.verse", "42");
    assert_project_check_error(
        root.join("BadRole\\BadRole.vproject"),
        "unknown package role `DefinitelyNotARole`",
    );

    write_project_file(
        &root,
        "BadVersion\\BadVersion.vproject",
        "package = BadVersion\nentry = main.verse\nrole = PersistenceCompatConstraint\nverseVersion = 99\n",
    );
    write_project_file(&root, "BadVersion\\main.verse", "42");
    assert_project_check_error(
        root.join("BadVersion\\BadVersion.vproject"),
        "unsupported Verse version `99`",
    );
}

#[test]
#[ignore = "planned Persistence/evolution FT column"]
fn checks_persistence_evolution_column_scope_remapped_schema_paths() {
    let root = temp_project_dir("persistence_evolution_scope_remap");
    write_profile_project(
        &root,
        "PublishedProfile",
        Some("PersistenceCompatConstraint"),
        r#"
Data<public> := module:
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
    var Saved:weak_map(player, profile_data) = map{}
"#,
    );
    write_profile_project(
        &root,
        "Game",
        None,
        r#"
PlayerData<public> := module:
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
        Coins<public>:int = 0
    var Saved:weak_map(player, profile_data) = map{}
PlayerData.profile_data{XP := 40, Coins := 2}.XP + 2
"#,
    );
    write_project_file(
        &root,
        "Game\\Game.vproject",
        "package = Game\nentry = main.verse\ndependencyPackages = PublishedProfile\npersistenceScopeRemap = PublishedProfile.Data:Game.PlayerData\n",
    );

    assert_eq!(
        check_project_file(root.join("Game\\Game.vproject")).expect("project should check"),
        Type::Int
    );
}

#[test]
#[ignore = "planned Persistence/evolution FT column"]
fn checks_persistence_evolution_column_backward_compatible_schema_changes() {
    let root = temp_project_dir("persistence_evolution_schema_changes");
    write_profile_project(
        &root,
        "PublishedProfile",
        Some("PersistenceCompatConstraint"),
        r#"
Profile<public> := module:
    rank<public> := enum<persistable>{Bronze, Silver}
    snapshot<public> := struct<persistable>:
        Rank<public>:rank = rank.Bronze
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
        Snapshot<public>:snapshot = snapshot{}
    var Saved:weak_map(player, profile_data) = map{}
"#,
    );
    write_profile_project(
        &root,
        "Game",
        None,
        r#"
Profile<public> := module:
    rank<public> := enum<persistable>{Bronze, Silver, Gold}
    snapshot<public> := struct<persistable>:
        Rank<public>:rank = rank.Bronze
        Wins<public>:int = 0
    profile_data<public> := class<final><persistable>:
        XP<public>:int = 0
        Snapshot<public>:snapshot = snapshot{}
        Title<public>:string = ""
    var Saved:weak_map(player, profile_data) = map{}
Profile.profile_data{XP := 42}.XP
"#,
    );
    write_project_file(
        &root,
        "Game\\Game.vproject",
        "package = Game\nentry = main.verse\ndependencyPackages = PublishedProfile\n",
    );

    assert_eq!(
        check_project_file(root.join("Game\\Game.vproject")).expect("project should check"),
        Type::Int
    );

    write_project_file(
        &root,
        "Game\\main.verse",
        r#"
Profile<public> := module:
    rank<public> := enum<persistable>{Bronze}
    snapshot<public> := struct<persistable>:
        Rank<public>:int = 0
    profile_data<public> := class<final><persistable>:
        XP<public>:float = 0.0
        Snapshot<public>:snapshot = snapshot{}
    var Saved:weak_map(player, profile_data) = map{}
42
"#,
    );
    assert_project_check_error(
        root.join("Game\\Game.vproject"),
        "is not backward-compatible with persistence constraint package `PublishedProfile`",
    );
}
