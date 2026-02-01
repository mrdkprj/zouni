use crate::{
    dialog::{message, MessageDialogOptions},
    fs::{clean_up, readdir, register_cancellable},
    platform::linux::{
        util::init,
        widgets::{create_progress_dialog, create_replace_confirm_dialog, FileOperationDialog, ReplaceOrSkip},
    },
};
use gtk::{
    gio::{prelude::CancellableExtManual, prelude::FileExtManual, traits::CancellableExt, traits::FileExt, Cancellable, File, FileCopyFlags, FileMeasureFlags, FileQueryInfoFlags, IOErrorEnum},
    glib::Priority,
};
use smol::{channel::Sender, stream::StreamExt};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FileOperation {
    Copy,
    Move,
    Delete,
    Trash,
}

enum BatchOpMessage {
    Ready,
    Started(String),
    Progress(i64, i64),
    Done(Result<(), String>),
    Finished,
}

pub(crate) fn execute_file_operation<P1: AsRef<Path>, P2: AsRef<Path>>(operation: FileOperation, froms: &[P1], to: Option<P2>) -> Result<(), String> {
    if froms.is_empty() {
        return Ok(());
    }

    init();

    let froms = froms.iter().map(|a| a.as_ref().to_path_buf()).collect::<Vec<_>>();
    let to = if let Some(to) = to {
        to.as_ref().to_path_buf()
    } else {
        PathBuf::new()
    };

    let (cancel_id, cancellable) = register_cancellable();

    let (tx, rx) = smol::channel::unbounded::<BatchOpMessage>();
    let (usage_tx, usage_rx) = smol::channel::bounded::<DiskUsages>(1);
    let (pause_tx, pause_rx) = smol::channel::bounded::<bool>(1);

    let widget = create_progress_dialog(&operation, "Preparing...", to.to_str().unwrap(), cancel_id, pause_tx);
    widget.show();

    gtk::glib::spawn_future_local(async move {
        let mut usages = usage_rx.recv().await.expect("Calculation failed");
        loop {
            if let Ok(result) = rx.recv().await {
                match result {
                    BatchOpMessage::Ready => {
                        widget.progress(0.0);
                        update_progress(&widget, &operation, &mut usages);
                    }
                    BatchOpMessage::Started(file) => {
                        widget.set_from_name(&file);
                    }
                    BatchOpMessage::Progress(proccessed, total) => {
                        if proccessed < total {
                            usages.processed_size += (total - proccessed) as u64;
                        } else {
                            usages.processed_size += total as u64;
                        }

                        update_progress(&widget, &operation, &mut usages);
                    }
                    BatchOpMessage::Done(result) => {
                        if result.is_err() {
                            let _ = smol::spawn(async move {
                                message(MessageDialogOptions {
                                    title: None,
                                    kind: Some(crate::dialog::MessageDialogKind::Error),
                                    buttons: vec!["OK".to_string()],
                                    message: result.err().unwrap(),
                                    cancel_id: None,
                                })
                                .await;
                            });
                        } else {
                            usages.processed_count += 1;
                            update_progress(&widget, &operation, &mut usages);
                        }
                    }
                    BatchOpMessage::Finished => {
                        clean_up(&widget, cancel_id);
                        break;
                    }
                }
            }
        }
    });

    gtk::glib::spawn_future_local(async move {
        let mut usages = DiskUsages::default();
        measure_size(&froms, &mut usages).await.expect("Calculation failed");

        usage_tx.send(usages).await.expect("Calculation failed");
        tx.send(BatchOpMessage::Ready).await.expect("Cannot start operation");

        let mut needs_confirm = Vec::new();
        for from in froms {
            let _ = tx.try_send(BatchOpMessage::Started(from.file_name().unwrap().to_string_lossy().to_string()));

            if cancellable.is_cancelled() {
                break;
            }

            if let Ok(pause) = pause_rx.try_recv() {
                if pause {
                    let _ = pause_rx.recv().await;
                }
            }

            match operation {
                FileOperation::Copy => execute_copy(from, to.clone(), &cancellable, &tx, &mut needs_confirm).await,
                FileOperation::Move => execute_move(from, to.clone(), &cancellable, &tx, None, &mut needs_confirm).await,
                FileOperation::Delete => execute_delete(from, &cancellable, &tx).await,
                FileOperation::Trash => execute_trash(from, &cancellable, &tx).await,
            }
        }

        if !needs_confirm.is_empty() {
            let mut replace_all = false;
            let dialog = create_replace_confirm_dialog(cancel_id);

            for file in needs_confirm {
                let _ = tx.try_send(BatchOpMessage::Started(file.file_name().unwrap().to_string_lossy().to_string()));

                if cancellable.is_cancelled() {
                    break;
                }

                if let Ok(pause) = pause_rx.try_recv() {
                    if pause {
                        let _ = pause_rx.recv().await;
                    }
                }

                let result = if replace_all {
                    ReplaceOrSkip::Replace
                } else {
                    dialog.confirm(&file).await
                };

                if result == ReplaceOrSkip::SkipAll {
                    break;
                }

                if result == ReplaceOrSkip::ReplaceAll {
                    replace_all = true;
                }

                if result == ReplaceOrSkip::Replace {
                    match operation {
                        FileOperation::Copy => execute_copy_force(file, to.clone(), &cancellable, &tx).await,
                        FileOperation::Move => execute_move_force(file, to.clone(), &cancellable, &tx, None).await,
                        _ => {}
                    }
                }
            }
        }

        let _ = tx.send(BatchOpMessage::Finished).await;
    });

    Ok(())
}

#[derive(Default, Debug)]
struct DiskUsages {
    total_size: u64,
    total_count: u64,
    processed_count: u64,
    processed_size: u64,
    progress: f64,
}

async fn measure_size(entries: &[PathBuf], usages: &mut DiskUsages) -> Result<(), String> {
    for entry in entries {
        if entry.is_dir() {
            let children = File::for_path(entry).enumerate_children_future("standard:name", FileQueryInfoFlags::NONE, Priority::DEFAULT).await.map_err(|e| e.message().to_string())?;
            let children: Vec<PathBuf> = children.filter_map(|info| info.ok()).map(|info| entry.join(info.name())).collect();
            Box::pin(measure_size(&children, usages)).await?;
        } else {
            let (disk_usage, _, num_files) = File::for_path(entry).measure_disk_usage_future(FileMeasureFlags::APPARENT_SIZE, Priority::DEFAULT).0.await.map_err(|e| e.message().to_string())?;
            usages.total_size += disk_usage;
            usages.total_count += num_files;
        }
    }
    Ok(())
}

fn update_progress(widget: &FileOperationDialog, operation: &FileOperation, usages: &mut DiskUsages) {
    let (messag, progress) = match operation {
        FileOperation::Copy => ("Copying", usages.processed_size as f64 / usages.total_size as f64),
        FileOperation::Move => ("Moving", usages.processed_size as f64 / usages.total_size as f64),
        FileOperation::Delete => ("Deleting", usages.processed_count as f64 / usages.total_count as f64),
        FileOperation::Trash => ("Trashing", usages.processed_count as f64 / usages.total_count as f64),
    };
    usages.progress = progress;
    let percent = usages.progress * 100.0;
    widget.set_title(&format!("{}% complete", percent.ceil().to_string()));
    widget.progress(usages.progress);

    widget.set_message(&format!("{messag} {}/{} items ", usages.processed_count.to_string(), usages.total_count.to_string()));
}

async fn run_with_cancellable<F, T>(
    operation: F,
    progress_stream: Option<Pin<Box<dyn smol::prelude::Stream<Item = (i64, i64)>>>>,
    cancellable: &Cancellable,
    tx: &Sender<BatchOpMessage>,
    cleanup_file: Option<File>,
    parent_dir: Option<PathBuf>,
) where
    F: smol::future::FutureExt<Output = Result<T, gtk::glib::Error>>,
{
    let progress_tx = tx.clone();

    if let Some(mut progress) = progress_stream {
        gtk::glib::spawn_future_local(async move {
            while let Some((current, total)) = progress.next().await {
                let _ = progress_tx.try_send(BatchOpMessage::Progress(current, total));
            }
        });
    }

    let cancellation_signal = async {
        cancellable.future().await;
        Err(gtk::glib::Error::new(IOErrorEnum::Cancelled, "User cancelled"))
    };

    match operation.race(cancellation_signal).await {
        Ok(_) => {
            let _ = tx.try_send(BatchOpMessage::Done(Ok(())));
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

            let _ = tx.try_send(BatchOpMessage::Done(Err(e.message().to_string())));
        }
    }
}

async fn execute_move(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>, parent: Option<PathBuf>, needs_confirm: &mut Vec<PathBuf>) {
    let source = File::for_parse_name(from.to_str().unwrap());
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_parse_name(dest_path.to_str().unwrap());

    // The native implementation may support moving directories (for instance on moves inside the same filesystem), but the fallback code does not.
    if from.is_dir() {
        return handle_directory(false, from, to, cancellable, tx, needs_confirm).await;
    }

    if dest_path.exists() {
        needs_confirm.push(from);
        return;
    }

    let (output, progress_stream) = source.move_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), parent).await;
}

async fn execute_move_force(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>, parent: Option<PathBuf>) {
    let source = File::for_parse_name(from.to_str().unwrap());
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_parse_name(dest_path.to_str().unwrap());

    let (output, progress_stream) = source.move_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), parent).await;
}

async fn execute_copy(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>, needs_confirm: &mut Vec<PathBuf>) {
    let source = File::for_parse_name(from.to_str().unwrap());
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_parse_name(dest_path.to_str().unwrap());

    // Can not handle recursive copies of directories
    if from.is_dir() {
        return handle_directory(true, from, to, cancellable, tx, needs_confirm).await;
    }

    if dest_path.exists() {
        needs_confirm.push(from);
        return;
    }

    let (output, progress_stream) = source.copy_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), None).await;
}

async fn execute_copy_force(from: PathBuf, to: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>) {
    let source = File::for_parse_name(from.to_str().unwrap());
    let dest_path = to.join(from.file_name().unwrap());
    let dest = File::for_parse_name(dest_path.to_str().unwrap());

    let (output, progress_stream) = source.copy_future(&dest, FileCopyFlags::ALL_METADATA | FileCopyFlags::NOFOLLOW_SYMLINKS | FileCopyFlags::OVERWRITE, Priority::DEFAULT);
    run_with_cancellable(output, Some(progress_stream), cancellable, tx, Some(dest), None).await;
}

async fn handle_directory(is_copy: bool, from: PathBuf, to: PathBuf, cancellable: &Cancellable, sender: &Sender<BatchOpMessage>, needs_confirm: &mut Vec<PathBuf>) {
    let source = File::for_parse_name(from.to_str().unwrap());
    let to_dr = to.join(from.file_name().unwrap());
    let dest = File::for_parse_name(to_dr.to_str().unwrap());

    if !dest.query_exists(Cancellable::NONE) {
        match dest.make_directory(Cancellable::NONE) {
            Ok(()) => {}
            Err(e) => {
                let _ = sender.try_send(BatchOpMessage::Done(Err(e.message().to_string())));
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
                Box::pin(execute_copy(from_file, to_dr.clone(), cancellable, sender, needs_confirm)).await;
            } else {
                Box::pin(execute_move(from_file, to_dr.clone(), cancellable, sender, Some(from.to_path_buf()), needs_confirm)).await;
            }
        }
    }
}

async fn execute_delete(file_path: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>) {
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

async fn execute_trash(file_path: PathBuf, cancellable: &Cancellable, tx: &Sender<BatchOpMessage>) {
    let file = File::for_path(file_path);
    let output = file.trash_future(Priority::DEFAULT);
    run_with_cancellable(output, None, cancellable, tx, None, None).await;
}
