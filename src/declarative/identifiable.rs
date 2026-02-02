use std::hash::Hash;

/// Identifiable trait - 为数据提供唯一标识符
///
/// 类似于 SwiftUI 的 Identifiable 协议，这个 trait 允许 ForEach
/// 跟踪列表中的元素，即使它们的位置发生变化。这对于高效的差异更新至关重要。
///
/// # 示例
///
/// ```rust
/// use chatty::declarative::Identifiable;
///
/// struct User {
///     id: usize,
///     name: String,
/// }
///
/// impl Identifiable for User {
///     type Id = usize;
///
///     fn id(&self) -> Self::Id {
///         self.id
///     }
/// }
/// ```
///
/// # 性能考虑
///
/// - ID 应该是稳定的（同一个对象始终返回同一个 ID）
/// - ID 应该是唯一的（不同对象有不同的 ID）
/// - ID 比较应该快速（通常是整数或字符串）
///
pub trait Identifiable {
    /// ID 类型（必须支持相等比较、哈希、克隆和线程安全）
    type Id: Eq + Hash + Clone + Send + Sync;

    /// 返回此对象的唯一标识符
    fn id(&self) -> Self::Id;
}

// 为常见类型实现 Identifiable

impl Identifiable for String {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.clone()
    }
}

impl Identifiable for &str {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.to_string()
    }
}

impl Identifiable for i32 {
    type Id = i32;

    fn id(&self) -> Self::Id {
        *self
    }
}

impl Identifiable for i64 {
    type Id = i64;

    fn id(&self) -> Self::Id {
        *self
    }
}

impl Identifiable for u32 {
    type Id = u32;

    fn id(&self) -> Self::Id {
        *self
    }
}

impl Identifiable for u64 {
    type Id = u64;

    fn id(&self) -> Self::Id {
        *self
    }
}

impl Identifiable for usize {
    type Id = usize;

    fn id(&self) -> Self::Id {
        *self
    }
}

// 为元组实现 Identifiable（用于快速原型）
impl<T: Identifiable> Identifiable for (T::Id, T) {
    type Id = T::Id;

    fn id(&self) -> Self::Id {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_identifiable() {
        let s = "hello".to_string();
        assert_eq!(s.id(), "hello");
    }

    #[test]
    fn test_str_identifiable() {
        let s = "world";
        assert_eq!(s.id(), "world".to_string());
    }

    #[test]
    fn test_usize_identifiable() {
        let n = 42usize;
        assert_eq!(n.id(), 42);
    }

    #[test]
    #[allow(dead_code)]
    fn test_tuple_identifiable() {
        struct User {
            name: String,
        }

        impl Identifiable for User {
            type Id = usize;
            fn id(&self) -> Self::Id {
                0
            }
        }

        let tuple = (
            123usize,
            User {
                name: "Alice".to_string(),
            },
        );
        assert_eq!(tuple.id(), 123);
    }
}
