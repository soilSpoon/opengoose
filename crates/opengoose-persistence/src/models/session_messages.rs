use diesel::prelude::*;

use crate::schema::{messages, sessions};

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub session_key: &'a str,
    pub selected_model: Option<&'a str>,
}

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub session_key: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub author: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::{NewMessage, NewSession};

    #[test]
    fn test_new_session_construction() {
        let session = NewSession {
            session_key: "discord:guild:chan",
            selected_model: None,
        };

        assert_eq!(session.session_key, "discord:guild:chan");
        assert!(session.selected_model.is_none());
    }

    #[test]
    fn test_new_message_with_author() {
        let message = NewMessage {
            session_key: "key",
            role: "user",
            content: "hello",
            author: Some("alice"),
        };

        assert_eq!(message.role, "user");
        assert_eq!(message.author, Some("alice"));
    }

    #[test]
    fn test_new_message_without_author() {
        let message = NewMessage {
            session_key: "key",
            role: "assistant",
            content: "hi",
            author: None,
        };

        assert!(message.author.is_none());
    }
}
