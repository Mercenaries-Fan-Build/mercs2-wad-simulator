//! Bink 1 (`BIKi`) movies, as the shipped `Data\Movies\*.bik` files need them.
//!
//! Written only from `SPEC-audio.md` and `SPEC-video.md` beside this crate, which were read out of
//! the game's own `binkw32.dll` and the movie bytes. No other Bink decoder was consulted; ffmpeg was
//! used as a black box to check results.

pub mod audio;
pub mod container;
pub mod rgb;
pub mod video;

pub use audio::AudioDecoder;
pub use container::{AudioTrack, Movie};
pub use video::{Frame, VideoDecoder};
