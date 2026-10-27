//! Small pure helpers shared by commands.

#[cfg(test)]
mod tests;

/// Formats rows as left-aligned columns separated by two spaces.
/// The last column is not padded.
pub fn table<R, C>(rows: R) -> String
where
    R: IntoIterator<Item = Vec<C>>,
    C: AsRef<str>,
{
    let rows: Vec<Vec<C>> = rows.into_iter().collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.as_ref().chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    for row in &rows {
        let last = row.len().saturating_sub(1);
        for (column, cell) in row.iter().enumerate() {
            let cell = cell.as_ref();
            if column == last {
                out.push_str(cell);
            } else {
                let pad = widths[column] - cell.chars().count();
                out.push_str(cell);
                out.extend(std::iter::repeat_n(' ', pad + 2));
            }
        }
        out.push('\n');
    }
    out
}

/// `1 error`, `2 warnings`.
pub fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}
