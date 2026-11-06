//! Gameplay and startup, above the reusable client library.
// No `#![recursion_limit]` or other inner attribute horizon-mixins doesn't know here: the
// launcher's horizon-mixins then rejects every native mod targeting this crate (#8169).
#![forbid(unsafe_code)]
extern crate self as horizon_client;
pub use horizon_client_lib::*;
#[doc(hidden)]
pub mod __internals;
pub mod entry;

// The game's allocator lives in the library, not the executable: the prebuilt game (#589) is this crate as a
// dylib, and the allocator must be the one its code and the mods' executable share.
#[cfg(not(any(
    feature = "system-allocator",
    feature = "alloc-census",
    feature = "alloc-count"
)))]
#[global_allocator]
static ALLOCATOR: horizon_threads::Allocator = horizon_threads::ALLOCATOR;
#[cfg(all(feature = "alloc-census", not(feature = "system-allocator")))]
#[global_allocator]
static ALLOCATOR: horizon_threads::CensusAllocator = horizon_threads::CensusAllocator;
#[cfg(all(feature = "alloc-count", not(feature = "system-allocator")))]
#[global_allocator]
static ALLOCATOR: horizon_threads::alloc_count::CountingAllocator =
    horizon_threads::alloc_count::CountingAllocator;
pub mod game;
