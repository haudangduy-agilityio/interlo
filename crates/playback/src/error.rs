//! Crate error enum.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlaybackError {
    #[error("playback is not implemented yet")]
    NotImplemented,
    /// The queue holds `max` segments. The caller must slow the producer or
    /// tell the owner; the queue never drops a segment to make room.
    #[error("playback queue is full ({max} segments)")]
    QueueFull { max: usize },
    /// `mark_played` named a segment that is not at the front of the queue
    /// (already finished, expired by a hold, or out of order).
    #[error("segment {0} is not the next segment in the queue")]
    NotNextSegment(String),
}
