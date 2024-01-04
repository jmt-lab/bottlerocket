use crate::{error, Result};
use futures::{Future, Stream, StreamExt};
use snafu::ResultExt;
use std::path::{Path, PathBuf};
use tokio::fs::{self, DirEntry};
use tokio_stream::wrappers::ReadDirStream;

/// Returns a stream of files in a directory that match a predicate.
///
/// Returns an empty stream if the directory does not exist.
pub(crate) async fn find_files<Pa, Pr, Fut>(
    target_dir: Pa,
    mut predicate: Pr,
) -> impl Stream<Item = Result<PathBuf>>
where
    Pa: AsRef<Path>,
    Pr: FnMut(DirEntry) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    async_stream::stream! {
        let dir_info = fs::read_dir(&target_dir).await;
        match dir_info {
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    yield Err(e).context(error::TraverseDirectorySnafu {
                        directory: target_dir.as_ref().to_owned()
                    });
                }
            }
            Ok(dir_info) => {
                let mut dir_reader =
                    ReadDirStream::new(dir_info);

                while let Some(dir_entry) = dir_reader.next().await {
                    let dir_entry = dir_entry.context(error::TraverseDirectorySnafu {
                        directory: target_dir.as_ref().to_owned()
                    })?;

                    let entry_path = dir_entry.path();
                    if predicate(dir_entry).await? {
                        yield Ok(entry_path)
                    }
                }
            }
        }
    }
}
