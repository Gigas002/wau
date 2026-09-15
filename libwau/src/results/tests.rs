use super::*;
use crate::model::Strategy;

#[test]
fn pkg_already_installed_message() {
    assert_eq!(
        ManagerError::PkgAlreadyInstalled.to_string(),
        "package already installed"
    );
}

#[test]
fn pkg_conflicts_with_installed_pluralises_for_multiple() {
    let one = ManagerError::PkgConflictsWithInstalled {
        conflicting: vec![PkgRef {
            source: "curse".into(),
            id: "1".into(),
            name: "Foo".into(),
        }],
    };
    assert_eq!(
        one.to_string(),
        "package folders conflict with installed package Foo (curse:1)"
    );

    let many = ManagerError::PkgConflictsWithInstalled {
        conflicting: vec![
            PkgRef {
                source: "curse".into(),
                id: "1".into(),
                name: "Foo".into(),
            },
            PkgRef {
                source: "wowi".into(),
                id: "2".into(),
                name: "Bar".into(),
            },
        ],
    };
    assert_eq!(
        many.to_string(),
        "package folders conflict with installed packages Foo (curse:1), Bar (wowi:2)"
    );
}

#[test]
fn pkg_conflicts_with_unreconciled_lists_folders() {
    let err = ManagerError::PkgConflictsWithUnreconciled {
        folders: vec!["Foo".into(), "Bar".into()],
    };
    assert_eq!(
        err.to_string(),
        "package folders conflict with 'Foo', 'Bar'"
    );
}

#[test]
fn pkg_files_missing_default_reason() {
    assert_eq!(
        ManagerError::files_missing().to_string(),
        "no files are available for download"
    );
}

#[test]
fn pkg_files_missing_custom_reason() {
    let err = ManagerError::PkgFilesMissing {
        reason: "package distribution is forbidden".into(),
    };
    assert_eq!(err.to_string(), "package distribution is forbidden");
}

#[test]
fn pkg_files_not_matching_formats_strategies() {
    let err = ManagerError::PkgFilesNotMatching {
        strategies: crate::model::Strategies {
            any_flavour: true,
            any_release_type: false,
            version_eq: Some("1.2.3".into()),
        },
    };
    assert_eq!(
        err.to_string(),
        "no files found for: any_flavour=True; version_eq=\"1.2.3\""
    );
}

#[test]
fn pkg_source_disabled_with_and_without_reason() {
    assert_eq!(
        ManagerError::PkgSourceDisabled { reason: None }.to_string(),
        "package source is disabled"
    );
    assert_eq!(
        ManagerError::PkgSourceDisabled {
            reason: Some("missing access token".into())
        }
        .to_string(),
        "package source is disabled: missing access token"
    );
}

#[test]
fn pkg_up_to_date_pinned_vs_not() {
    assert_eq!(
        ManagerError::PkgUpToDate { is_pinned: true }.to_string(),
        "package is pinned"
    );
    assert_eq!(
        ManagerError::PkgUpToDate { is_pinned: false }.to_string(),
        "package is up to date"
    );
}

#[test]
fn pkg_strategies_unsupported_sorts_and_joins() {
    let err = ManagerError::PkgStrategiesUnsupported {
        strategies: vec![Strategy::VersionEq, Strategy::AnyFlavour],
    };
    assert_eq!(
        err.to_string(),
        "strategies are not valid for source: any_flavour, version_eq"
    );
}

#[test]
fn internal_error_message_format() {
    let err = InternalError::new("connection reset");
    assert_eq!(err.to_string(), "internal error: \"connection reset\"");
}

#[test]
fn failure_from_manager_error_transparent_display() {
    let failure: Failure = ManagerError::PkgNonexistent.into();
    assert_eq!(failure.to_string(), "package does not exist");
}

#[test]
fn failure_from_internal_error_transparent_display() {
    let failure: Failure = InternalError::new("boom").into();
    assert_eq!(failure.to_string(), "internal error: \"boom\"");
}
