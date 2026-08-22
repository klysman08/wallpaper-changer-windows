//! Running independent per-image work across a bounded number of threads.
//!
//! Two places need exactly this: fitting a collage's cells, and decoding a batch of
//! thumbnails. Both are a list of independent jobs whose cost is dominated by decoding
//! and resampling, and both were serial — which is what put the port behind the Python
//! it replaced, since Pillow does the same work in optimised C.
//!
//! ## Why not rayon
//!
//! A global work-stealing pool is more than this needs. There are exactly two call
//! sites, both handed a short list from a request that is already on a blocking
//! thread, and `std::thread::scope` borrows the inputs without any of them having to
//! be `'static`. The dependency is not worth the two functions it would save.
//!
//! ## Why the worker count is small
//!
//! [`MAX_WORKERS`] is 4, not the core count. Each worker holds a **decoded, full-size
//! image** while it works — sixty megabytes for a large photograph, before the output
//! it is producing. Memory was half of what made this worth fixing, so the cap trades
//! some throughput on a machine with many cores for a peak that stays bounded no
//! matter how big the batch or the pictures are.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::CoreError;

/// The most threads any batch will use at once. See the module docs.
pub(crate) const MAX_WORKERS: usize = 4;

/// Apply `f` to every item across at most [`MAX_WORKERS`] threads, results in order.
///
/// Order is restored before returning, so a caller can still report the *first*
/// failure in its own terms rather than whichever thread happened to lose first. That
/// matters more than it sounds: without it, the same broken folder produces a
/// different error message on different runs.
pub(crate) fn map_bounded<T, R>(
    items: &[T],
    f: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>, CoreError>
where
    T: Sync,
    R: Send,
{
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(MAX_WORKERS)
        .min(items.len())
        .max(1);

    // One item, or one core: no threads worth the setup.
    if workers == 1 {
        return Ok(items.iter().map(&f).collect());
    }

    let next = AtomicUsize::new(0);
    let mut gathered: Vec<(usize, R)> = Vec::with_capacity(items.len());
    let mut lost = false;

    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let f = &f;
                let next = &next;
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            return mine;
                        };
                        mine.push((at, f(item)));
                    }
                })
            })
            .collect();
        for handle in handles {
            match handle.join() {
                Ok(mine) => gathered.extend(mine),
                // Dropping a worker's results silently would leave holes the caller
                // would index into. Say so instead.
                Err(_) => lost = true,
            }
        }
    });

    if lost {
        return Err(CoreError::internal(
            "A worker panicked while processing images.",
        ));
    }
    gathered.sort_by_key(|(at, _)| *at);
    Ok(gathered.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_input_order() {
        // The jobs finish out of order on purpose: the early ones sleep longest, so a
        // naive "collect as they complete" would reverse them.
        let items: Vec<u64> = (0..16).collect();
        let out = map_bounded(&items, |n| {
            std::thread::sleep(std::time::Duration::from_millis(16 - *n));
            *n * 2
        })
        .unwrap();
        assert_eq!(out, (0..16).map(|n| n * 2).collect::<Vec<u64>>());
    }

    #[test]
    fn an_empty_batch_is_not_an_error() {
        let items: Vec<u64> = Vec::new();
        assert!(map_bounded(&items, |n| *n).unwrap().is_empty());
    }

    #[test]
    fn a_panicking_job_is_reported_rather_than_leaving_a_hole() {
        let items: Vec<u64> = (0..8).collect();
        let out = map_bounded(&items, |n| {
            assert_ne!(*n, 5, "deliberate");
            *n
        });
        assert_eq!(out.unwrap_err().kind(), crate::ErrorKind::Internal);
    }
}
