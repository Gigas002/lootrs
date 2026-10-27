use super::*;

#[test]
fn table_aligns_columns() {
    let out = table([
        vec!["NAME", "RUNNER"],
        vec!["witcher3", "umu"],
        vec!["wow", "wine"],
    ]);

    assert_eq!(out, "NAME      RUNNER\nwitcher3  umu\nwow       wine\n");
}

#[test]
fn table_counts_chars_not_bytes() {
    let out = table([vec!["ü", "x"], vec!["ab", "y"]]);

    assert_eq!(out, "ü   x\nab  y\n");
}

#[test]
fn empty_table() {
    assert_eq!(table(Vec::<Vec<&str>>::new()), "");
}

#[test]
fn plural_forms() {
    assert_eq!(plural(0, "error"), "0 errors");
    assert_eq!(plural(1, "error"), "1 error");
    assert_eq!(plural(2, "warning"), "2 warnings");
    assert_eq!(plural(3, "process"), "3 processes");
}
