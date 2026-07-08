//! AT-SPI window-id hashing.
//!
//! AT-SPI accessibles are identified by a D-Bus (bus name, object path) pair,
//! not a numeric id like macOS's `CGWindowID`. We synthesise a `u32`
//! `WindowId` by hashing that pair. The hash is deterministic within a process
//! run, which is all `read_window_text` needs: it re-enumerates and re-hashes
//! in the same process to resolve an id back to its accessible (stateless
//! resolve-by-id, mirroring the macOS backend).

/// Stable FNV-1a hash of an AT-SPI accessible's identity -> a `u32` window id.
pub fn window_id_hash(bus_name: &str, object_path: &str) -> u32 {
    const OFFSET: u32 = 2166136261;
    const PRIME: u32 = 16777619;
    let mut hash = OFFSET;
    // The NUL separator keeps (a, bc) and (ab, c) from colliding.
    for byte in bus_name
        .bytes()
        .chain(std::iter::once(0))
        .chain(object_path.bytes())
    {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_distinguishes_paths() {
        let a = window_id_hash(":1.42", "/org/a11y/atspi/accessible/1");
        let b = window_id_hash(":1.42", "/org/a11y/atspi/accessible/1");
        let c = window_id_hash(":1.42", "/org/a11y/atspi/accessible/2");
        assert_eq!(a, b, "same identity must hash equal");
        assert_ne!(a, c, "different object paths must differ");
    }

    #[test]
    fn separator_prevents_boundary_collision() {
        // Without the NUL separator these two identities would hash identically.
        let a = window_id_hash(":1.4", "2/path");
        let b = window_id_hash(":1.42", "/path");
        assert_ne!(a, b, "bus/path boundary must be significant");
    }
}
