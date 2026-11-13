use serde::{Deserialize, Serialize};

/// 跟随配置分享的窗口范围；不存每次启动都会变化的 PID、句柄或本机路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowBinding {
    pub exe: String,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub title_contains: Option<String>,
}

impl WindowBinding {
    pub fn sanitized(mut self) -> Option<Self> {
        fn clean(value: &str, max: usize) -> String {
            value
                .chars()
                .filter(|c| !c.is_control())
                .take(max)
                .collect::<String>()
                .trim()
                .to_string()
        }
        self.exe = clean(&self.exe, 260);
        if self.exe.is_empty() || self.exe.contains(['/', '\\', ':']) {
            return None;
        }
        self.class = self.class.map(|s| clean(&s, 256)).filter(|s| !s.is_empty());
        self.title_contains = self
            .title_contains
            .map(|s| clean(&s, 256))
            .filter(|s| !s.is_empty());
        Some(self)
    }

    pub fn matches(&self, exe: Option<&str>, class: &str, title: &str) -> bool {
        let identity = match exe {
            Some(exe) => {
                self.exe.eq_ignore_ascii_case(exe)
                    && self.class.as_ref().is_none_or(|expected| expected == class)
            }
            None => self
                .class
                .as_ref()
                .is_some_and(|expected| !expected.is_empty() && expected == class),
        };
        identity
            && self
                .title_contains
                .as_ref()
                .is_none_or(|s| title.contains(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> WindowBinding {
        WindowBinding {
            exe: "Game.exe".into(),
            class: Some("GameWindow".into()),
            title_contains: None,
        }
    }

    #[test]
    fn requires_exe_and_optional_class_but_uses_class_if_query_denied() {
        let binding = binding();
        assert!(binding.matches(Some("GAME.EXE"), "GameWindow", "角色"));
        assert!(!binding.matches(Some("other.exe"), "GameWindow", "角色"));
        assert!(!binding.matches(Some("game.exe"), "Other", "角色"));
        assert!(binding.matches(None, "GameWindow", "角色"));
        assert!(!binding.matches(None, "Other", "角色"));
        assert!(!WindowBinding {
            class: None,
            ..binding
        }
        .matches(None, "GameWindow", "角色"));
    }

    #[test]
    fn title_restriction_and_invalid_identifiers_fail_closed() {
        let restricted = WindowBinding {
            title_contains: Some("角色甲".into()),
            ..binding()
        };
        assert!(restricted.matches(Some("game.exe"), "GameWindow", "剑网三 - 角色甲"));
        assert!(!restricted.matches(Some("game.exe"), "GameWindow", "角色乙"));
        assert!(WindowBinding {
            exe: "C:\\game.exe".into(),
            ..binding()
        }
        .sanitized()
        .is_none());
        assert!(WindowBinding {
            exe: " \n".into(),
            ..binding()
        }
        .sanitized()
        .is_none());
        assert_eq!(
            WindowBinding {
                exe: " game.exe\n".into(),
                ..binding()
            }
            .sanitized()
            .unwrap()
            .exe,
            "game.exe"
        );
    }
}
