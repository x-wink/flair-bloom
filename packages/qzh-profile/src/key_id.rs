//! 统一的"按键标识"：键盘 VK 码与鼠标按钮的 tagged union。
//!
//! 取代旧 schema v1 中裸用 `u32` 表示按键的字段。前后端通过相同 wire format
//! 共享：`{"kind":"keyboard","code":81}` / `{"kind":"mouse","code":"left"}`。

use serde::{Deserialize, Serialize};

/// 鼠标按钮（Win32 物理映射顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    /// 鼠标左键
    Left,
    /// 鼠标右键
    Right,
    /// 鼠标中键（滚轮按下）
    Middle,
    /// 侧键 1（XBUTTON1）
    X1,
    /// 侧键 2（XBUTTON2）
    X2,
    /// 滚轮向上（注入时每次触发一格，触发时瞬发）
    WheelUp,
    /// 滚轮向下
    WheelDown,
}

/// 按键标识：要么是 Win32 虚拟键码（键盘），要么是某个鼠标按钮。
///
/// `Hash`/`Eq` 派生使其可作为 `HashMap` / `HashSet` 的键，配合 `PENDING_INJECTIONS`
/// 等等价机制使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum KeyId {
    /// 键盘按键，存放 Win32 VK 码。
    Keyboard(u32),
    /// 鼠标按钮。
    Mouse(MouseButton),
}

impl KeyId {
    /// 把旧 schema v1 中的裸 VK 码包装为键盘 [`KeyId`]。
    pub fn keyboard(vk: u32) -> Self {
        Self::Keyboard(vk)
    }

    /// 是否是鼠标按钮。
    pub fn is_mouse(&self) -> bool {
        matches!(self, Self::Mouse(_))
    }

    /// 是否是鼠标滚轮（瞬发事件，无法「按住」，故 Hold 规则对其只能每格点按一次）。
    pub fn is_wheel(&self) -> bool {
        matches!(
            self,
            Self::Mouse(MouseButton::WheelUp | MouseButton::WheelDown)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keyboard_round_trip() {
        let k = KeyId::Keyboard(0x51);
        let v = serde_json::to_value(k).unwrap();
        assert_eq!(v, json!({"kind":"keyboard","code":81}));
        let back: KeyId = serde_json::from_value(v).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn mouse_left_round_trip() {
        let k = KeyId::Mouse(MouseButton::Left);
        let v = serde_json::to_value(k).unwrap();
        assert_eq!(v, json!({"kind":"mouse","code":"left"}));
        let back: KeyId = serde_json::from_value(v).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn mouse_x1_x2_serialize() {
        let v = serde_json::to_value(KeyId::Mouse(MouseButton::X1)).unwrap();
        assert_eq!(v, json!({"kind":"mouse","code":"x1"}));
        let v = serde_json::to_value(KeyId::Mouse(MouseButton::X2)).unwrap();
        assert_eq!(v, json!({"kind":"mouse","code":"x2"}));
    }

    #[test]
    fn mouse_wheel_round_trip() {
        let v = serde_json::to_value(KeyId::Mouse(MouseButton::WheelUp)).unwrap();
        assert_eq!(v, json!({"kind":"mouse","code":"wheel_up"}));
        let back: KeyId = serde_json::from_value(v).unwrap();
        assert_eq!(back, KeyId::Mouse(MouseButton::WheelUp));

        let v = serde_json::to_value(KeyId::Mouse(MouseButton::WheelDown)).unwrap();
        assert_eq!(v, json!({"kind":"mouse","code":"wheel_down"}));
        let back: KeyId = serde_json::from_value(v).unwrap();
        assert_eq!(back, KeyId::Mouse(MouseButton::WheelDown));
    }

    #[test]
    fn keyid_is_hash_eligible() {
        let mut set = std::collections::HashSet::new();
        set.insert(KeyId::Keyboard(0x51));
        set.insert(KeyId::Mouse(MouseButton::Left));
        assert!(set.contains(&KeyId::Keyboard(0x51)));
        assert!(set.contains(&KeyId::Mouse(MouseButton::Left)));
    }
}
