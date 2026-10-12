//! Pure speech-output logic (see `README.md`).

pub mod normalize;
pub mod queue;
pub mod segment;

pub use normalize::{Lang, normalize};
pub use queue::{Action, Enqueued, Priority, SpeechQueue, Utterance};
pub use segment::{first_chunk_split, segments, split_sentences};
