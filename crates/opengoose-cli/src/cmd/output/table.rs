/// Format `headers` and `rows` as an aligned text table.
pub fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= widths.len() {
                widths.push(cell.len());
            } else {
                widths[idx] = widths[idx].max(cell.len());
            }
        }
    }

    let mut output = String::new();
    let header_row: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    push_table_row(&mut output, &header_row, &widths);

    let separator_len = widths.iter().sum::<usize>() + 2 * widths.len().saturating_sub(1);
    output.push_str(&"-".repeat(separator_len));
    output.push('\n');

    for row in rows {
        push_table_row(&mut output, row, &widths);
    }

    output
}

fn push_table_row(output: &mut String, row: &[String], widths: &[usize]) {
    for (idx, cell) in row.iter().enumerate() {
        if idx > 0 {
            output.push_str("  ");
        }
        let width = widths.get(idx).copied().unwrap_or_default();
        output.push_str(&format!("{cell:<width$}"));
    }
    output.push('\n');
}
