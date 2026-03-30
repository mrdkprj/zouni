use crate::{
    fs::{readdir, FileOperation, OperationStatus, Response, Total},
    platform::linux::util::init,
};
use gtk::{
    gio::{prelude::CancellableExtManual, prelude::FileExtManual, traits::CancellableExt, traits::FileExt, Cancellable, File, FileCopyFlags, FileMeasureFlags, FileQueryInfoFlags, IOErrorEnum},
    glib::Priority,
};
use smol::{
    channel::{Receiver, Sender},
    stream::StreamExt,
};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

pub(crate) fn execute_file_operation<F, P1: AsRef<Path>, P2: AsRef<Path>>(operation: FileOperation, froms: &[P1], to: Option<P2>, mut callback: F)
where
    F: AsyncFnMut(OperationStatus) -> Response + 'static,
{
    init();

    let froms = froms.iter().map(|a| a.as_ref().to_path_buf()).collect::<Vec<_>>();
    let to = if let Some(to) = to {
        to.as_ref().to_path_buf()
    } else {
        PathBuf::new()
    };

    let (tx, rx) = smol::channel::unbounded::<OperationStatus>();
    let (confirm_tx, confirm_rx) = smol::channel::bounded::<Response>(1);

    let cancellable = Cancellable::new();
    let ref_cancellable = cancellable.clone();

    gtk::glib::spawn_future_local(async move {
        loop {
            if let Ok(result) = rx.recv().await {
                match result {
                    OperationStatus::Confirm(_) => {
                        let response = callback(result).await;
                        match response {
                            Response::Cancel => {
                                cancellable.cancel();
                                break;
                            }
                            Response::Proceed => {
                                let _ = confirm_tx.send(Response::Replace).await;
                            }
                            _ => {
                                let _ = confirm_tx.send(response).await;
                            }
                        }
                    }
                    OperationStatus::Finished => {
                        let _ = callback(result).await;
                        break;
                    }
                    _ => {
                        if callback(result).await == Response::Cancel {
                            cancellable.cancel();
                            break;
                        }
                    }
                }
            }
        }
    });

    gtk::glib::spawn_future_local(async move {
        let mut total = Total::default();

        if measure_size(&froms, &mut total).await.is_err() {
            let _ = tx.send(OperationStatus::Error("Calculation failed".to_string()));
            return;
        }

        tx.send(OperationStatus::Ready(total)).await.expect("Cannot start operation");

        for from in froms {
            if ref_cancellable.is_cancelled() {
                break;
            }

            let _ = tx.send(OperationStatus::Start(from.file_name().unwrap().to_string_lossy().to_string())).await;

            match operation {
                FileOperation::Copy => execute_copy(from, to.clone(), &ref_cancellable, &tx, &confirm_rx).await,
                FileOperation::Move => execute_move(from, to.clone(), &ref_cancellable, &tx, None, &confirm_rx).await,
                FileOperation::Delete => execute_delete(from, &ref_cancellable, &tx).await,
                FileOperation::Trash => execute_trash(from, &ref_cancellable, &tx).await,
            }
        }

        let _ = tx.send(OperationStatus::Finished).await;
    });
}

async fn measure_size(entries: &[PathBuf], data: &mut Total) -> Result<(), String> {
    for entry in entries {
        if entry.is_dir() {
            let children = File::for_path(entry).enumerate_children_future("standard:name", FileQueryInfoFlags::NONE, Priority::DEFAULT).await.map_err(|e| e.message().to_string())?;
            let children: Vec<PathBuf> = children.filter_map(|info| info.ok()).map(|info| entry.join(info.name())).collect();
            Box::pin(measure_size(&children, data)).await?;
        } else {
            let (disk_usage, _, num_files) = File::for_path(entry).measure_disk_usage_future(FileMeasureFlags::APPARENT_SIZE, Priority::DEFAULT).0.await.map_err(|e| e.message().to_string())?;
            data.total_size += disk_usage;
            data.total_count += num_files;
        }
    }
    Ok(())
}

async fn run_with_cancellable<F, T>(
    operation: F,
    progress_stream: Option<Pin<Box<dyn smol::prelude::Stream<Item = (i64, i64)>>>>,
    cancellable: &Cancellable,
    tx: &Sender<OperationStatus>,
    cleanup_file: Option<File>,
    parent_dir: Option<PathBuf>,
) where
    F: smol::future::FutureExt<Output = Result<T, gtk::glib::Error>>,
{
    let progress_tx = tx.clone();

    if let Some(mut progress) = progress_stream {
        gtk::glib::spawn_future_local(async move {
            while let Some((current, total)) = progress.next().await {
                let _ = progress_tx.try_send(OperationStatus::Progress(current, total));
            }
        });
    }

    let cancellation_signal = async {
        cancellable.future().await;
        Err(gtk::glib::Error::new(IOErrorEnum::Cancelled, "User cancelled"))
    };

    match operation.race(cancellation_signal).await {
        Ok(_) => {
            let _ = tx.try_send(OperationStatus::End);
        }
        Err(e) => {
            // If cancelled, delete destination file that may be halfway
            if e.matches(IOErrorEnum::Cancelled) {
                if let Some(file) = cleanup_file {
                    let _ = file.delete_async(Priority::DEFAULT, Cancellable::NONE, |_| {});
                }
            }

            // If move, delete the remaining empty source directory
            if let Some(parent) = parent_dir {
                let _ = File::for_path(parent).delete_async(Priority::DEFAULT, Cancellable::NONE, |_| {});
            }

            let _ = tx.try_send(OperationStatus::Error(e.message().to_string()));
        }
    }
}

async fn execute_move(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<OperationStatus>, parent: Option<PathBuf>, confirm_rx: &Receiver<Response>) {
    let source = File::for_path(&from);
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_path(&dest_path);

    // The native implementation may support moving directories (for instance on moves inside the same filesystem), but the fallback code does not.
    if from.is_dir() {
        return handle_directory(false, from, to, cancellable, tx, confirm_rx).await;
    }

    if dest_path.exists() {
        let _ = tx.send(OperationStatus::Confirm(from.to_string_lossy().to_string())).await;
        let result = if let Ok(response) = confirm_rx.recv().await {
            response
        } else {
            Response::Skip
        };
        if result == Response::Skip {
            return;
        }
    }

    let (output, progress_stream) = source.move_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), parent).await;
}

async fn execute_copy(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<OperationStatus>, confirm_rx: &Receiver<Response>) {
    let source = File::for_path(&from);
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_path(&dest_path);

    // Can not handle recursive copies of directories
    if from.is_dir() {
        return handle_directory(true, from, to, cancellable, tx, confirm_rx).await;
    }

    if dest_path.exists() {
        let _ = tx.send(OperationStatus::Confirm(from.to_string_lossy().to_string())).await;
        let result = if let Ok(response) = confirm_rx.recv().await {
            response
        } else {
            Response::Skip
        };
        if result == Response::Skip {
            return;
        }
    }

    let (output, progress_stream) = source.copy_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), None).await;
}

async fn handle_directory(is_copy: bool, from: PathBuf, to: PathBuf, cancellable: &Cancellable, sender: &Sender<OperationStatus>, confirm_rx: &Receiver<Response>) {
    let source = File::for_path(&from);
    let to_dr = to.join(from.file_name().unwrap());
    let dest = File::for_path(&to_dr);

    if !dest.query_exists(Cancellable::NONE) {
        match dest.make_directory(Cancellable::NONE) {
            Ok(()) => {}
            Err(e) => {
                let _ = sender.try_send(OperationStatus::Error(e.message().to_string()));
            }
        };

        let settable_attributes = dest.query_settable_attributes(Cancellable::NONE).unwrap();
        let attributes_info = settable_attributes.attributes();
        let attributes = attributes_info.iter().map(|a| a.name()).collect::<Vec<&str>>().join(",");
        let info = source.query_info(&attributes, FileQueryInfoFlags::NONE, Cancellable::NONE).unwrap();
        dest.set_attributes_from_info(&info, FileQueryInfoFlags::NONE, Cancellable::NONE).unwrap();
    }

    if let Ok(mut children) = source.enumerate_children("standard:name", FileQueryInfoFlags::NONE, Cancellable::NONE) {
        while let Some(Ok(info)) = children.next() {
            let from_file = from.to_path_buf().join(info.name());
            if is_copy {
                Box::pin(execute_copy(from_file, to_dr.clone(), cancellable, sender, confirm_rx)).await;
            } else {
                Box::pin(execute_move(from_file, to_dr.clone(), cancellable, sender, Some(from.to_path_buf()), confirm_rx)).await;
            }
        }
    }
}

async fn execute_delete(file_path: PathBuf, cancellable: &Cancellable, tx: &Sender<OperationStatus>) {
    if file_path.is_dir() {
        if let Ok(files) = readdir(&file_path, false, false) {
            for file in files {
                Box::pin(execute_delete(PathBuf::from(file.full_path), cancellable, tx)).await;
            }
        }
    }

    let file = File::for_path(file_path);
    let output = file.delete_future(Priority::DEFAULT);
    run_with_cancellable(output, None, cancellable, tx, None, None).await;
}

async fn execute_trash(file_path: PathBuf, cancellable: &Cancellable, tx: &Sender<OperationStatus>) {
    let file = File::for_path(file_path);
    let output = file.trash_future(Priority::DEFAULT);
    run_with_cancellable(output, None, cancellable, tx, None, None).await;
}
