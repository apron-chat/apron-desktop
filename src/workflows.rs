//! Protocol request construction; server policy remains authoritative.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Create,
    Thread,
    Settings,
    AddMember,
    RemoveMember,
    Leave,
}
impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Create => "Create room",
            Self::Thread => "Create thread",
            Self::Settings => "Room settings",
            Self::AddMember => "Add member",
            Self::RemoveMember => "Remove member",
            Self::Leave => "Leave room",
        }
    }
    pub fn room_form(self) -> bool {
        matches!(self, Self::Create | Self::Thread | Self::Settings)
    }
    pub fn request(
        self,
        room: &Value,
        title: &str,
        description: &str,
        user: &str,
    ) -> Result<(&'static str, Value)> {
        let id = room["room_id"].as_str().unwrap_or("");
        ensure!(
            self == Self::Create || !id.is_empty(),
            "Choose a room first"
        );
        if self.room_form() {
            ensure!(
                !title.trim().is_empty()
                    && title.len() <= 200
                    && !title.chars().any(char::is_control),
                "Use a title of 1–200 bytes without control characters"
            );
            ensure!(description.len() <= 4000, "Description exceeds 4,000 bytes");
            let mut params = json!({"title":title.trim(),"description":description});
            match self {
                Self::Thread => {
                    params["parent_room_id"] = json!(id);
                    if room["private"] == true {
                        params["private"] = json!(true);
                    }
                }
                Self::Settings => {
                    params["room_id"] = json!(id);
                }
                _ => {}
            }
            Ok(("room_set", params))
        } else {
            let mut params = json!({"room_id":id});
            if self != Self::Leave {
                ensure!(
                    !user.trim().is_empty()
                        && user.len() <= 128
                        && !user.chars().any(char::is_control),
                    "Enter an exact user ID of 1–128 bytes"
                );
                params["user_id"] = json!(user.trim());
            }
            Ok((
                if self == Self::AddMember {
                    "room_join"
                } else {
                    "room_leave"
                },
                params,
            ))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_fixed_fields_and_uses_exact_membership_ids() {
        let room = json!({"room_id":"r","private":true,"parent_room_id":"p"});
        let (_, thread) = Kind::Thread.request(&room, "Topic", "About", "").unwrap();
        assert_eq!(thread["private"], true);
        assert_eq!(thread["parent_room_id"], "r");
        let (_, edit) = Kind::Settings.request(&room, "New", "About", "").unwrap();
        assert!(edit.get("parent_room_id").is_none());
        assert!(edit.get("private").is_none());
        assert!(Kind::AddMember.request(&room, "", "", "").is_err());
        let (method, params) = Kind::RemoveMember.request(&room, "", "", "alice").unwrap();
        assert_eq!(method, "room_leave");
        assert_eq!(params["user_id"], "alice");
    }
}
