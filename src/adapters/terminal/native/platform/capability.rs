pub(in crate::adapters::terminal::native) const LIVE_INPUT: bool =
    cfg!(any(target_os = "linux", target_os = "macos"));
