//! Every file in `examples/` must load and validate without errors.

use std::path::Path;

use liblootrs::{
    model::{Severity, Subject, ValidationContext},
    store::Catalog,
};

#[test]
fn examples_load_and_validate() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let catalog = Catalog::load(&examples);

    assert!(catalog.load_errors.is_empty(), "{:?}", catalog.load_errors);
    assert_eq!(catalog.games.len(), 3);
    assert_eq!(catalog.profiles.len(), 3);

    let ctx = ValidationContext {
        home: Path::new("/home/user"),
        battleye_runtime: false,
        eac_runtime: false,
    };
    let errors: Vec<_> = catalog
        .validate(&ctx)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        // Runners compiled out are expected errors in slimmer builds.
        .filter(|issue| !issue.message.contains("is not available"))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");

    let unavailable = catalog
        .validate(&ctx)
        .into_iter()
        .filter(|issue| issue.message.contains("is not available"))
        .count();
    let expected = [cfg!(feature = "wine"), cfg!(feature = "umu")]
        .iter()
        .filter(|available| !**available)
        .count();
    assert_eq!(unavailable, expected);
    assert!(
        catalog
            .validate(&ctx)
            .iter()
            .all(|issue| !matches!(issue.subject, Subject::File(_)))
    );
}
