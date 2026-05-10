const PREVIEW_PREFIX: &str = "Preview:";
const PREVIEW_BLOCK_PREFIX: &str = "\n\nPreview:";

pub(super) fn body_with_local_time(body: &str, formatted_time: &str) -> String {
    let body = body.trim_end();
    if body.is_empty() {
        return format!("Time: {formatted_time}");
    }

    if let Some((details, preview)) = split_trailing_preview(body) {
        let details = details.trim_end();
        if details.is_empty() {
            return format!("Time: {formatted_time}\n\n{preview}");
        }

        return format!("{details}\nTime: {formatted_time}\n\n{preview}");
    }

    format!("{body}\nTime: {formatted_time}")
}

fn split_trailing_preview(body: &str) -> Option<(&str, &str)> {
    if body.starts_with(PREVIEW_PREFIX) {
        return Some(("", body));
    }

    body.rfind(PREVIEW_BLOCK_PREFIX).map(|index| {
        let preview_index = index + 2;
        (&body[..index], &body[preview_index..])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_time_to_regular_body() {
        assert_eq!(
            body_with_local_time("Ready for review.", "2026-05-10 10:31:43 +08:00"),
            "Ready for review.\nTime: 2026-05-10 10:31:43 +08:00"
        );
    }

    #[test]
    fn places_time_before_trailing_preview_block() {
        assert_eq!(
            body_with_local_time(
                "Project: agents-notifier\nSession: agents-notifier sync report\nDuration: 2m 29s\nBranch: main\n\nPreview: The final answer preview...",
                "2026-05-10 10:31:43 +08:00",
            ),
            "Project: agents-notifier\nSession: agents-notifier sync report\nDuration: 2m 29s\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\n\nPreview: The final answer preview..."
        );
    }

    #[test]
    fn handles_preview_only_body() {
        assert_eq!(
            body_with_local_time(
                "Preview: The final answer preview...",
                "2026-05-10 10:31:43 +08:00"
            ),
            "Time: 2026-05-10 10:31:43 +08:00\n\nPreview: The final answer preview..."
        );
    }
}
