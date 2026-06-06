#[path = "profile_next/mod.rs"]
mod profile_next;

#[allow(unused_imports)]
pub use profile_next::{run, Event, InitArgs, ProfileArgs, ProfileCmd, ShowArgs, UpdateArgs};
