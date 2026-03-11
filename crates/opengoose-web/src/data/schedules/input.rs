pub(super) fn normalize_input(input: String, max_bytes: usize) -> String {
    if input.trim().is_empty() {
        String::new()
    } else {
        truncate_to_byte_boundary(&input, max_bytes)
    }
}

pub(super) fn normalize_trimmed_field(value: &str, max_bytes: usize) -> String {
    truncate_to_byte_boundary(value.trim(), max_bytes)
}

pub(super) fn normalize_optional_field(value: Option<&str>, max_bytes: usize) -> Option<String> {
    value
        .map(|item| normalize_trimmed_field(item, max_bytes))
        .filter(|item| !item.is_empty())
}

pub(super) fn trimmed_len_exceeds(value: &str, max_bytes: usize) -> bool {
    value.trim().len() > max_bytes
}

fn truncate_to_byte_boundary(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }

    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
