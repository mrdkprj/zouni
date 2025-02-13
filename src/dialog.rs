use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageDialogKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDialogOptions {
    pub title: Option<String>,
    pub kind: Option<MessageDialogKind>,
    pub buttons: Vec<String>,
    pub message: String,
    pub cancel_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OpenProperty {
    OpenFile,
    OpenDirectory,
    MultiSelections,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenDialogOptions {
    pub title: Option<String>,
    pub default_path: Option<String>,
    pub filters: Option<Vec<FileFilter>>,
    pub properties: Option<Vec<OpenProperty>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveDialogOptions {
    pub title: Option<String>,
    pub default_path: Option<String>,
    pub filters: Option<Vec<FileFilter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilter {
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            extensions: extensions.to_vec().iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDialogResult {
    pub canceled: bool,
    pub file_paths: Vec<String>,
}

impl Default for FileDialogResult {
    fn default() -> Self {
        Self {
            canceled: true,
            file_paths: Vec::new(),
        }
    }
}

const CANCEL: &str = "Cancel";
const NO: &str = "No";

fn get_level(kind: &Option<MessageDialogKind>) -> MessageLevel {
    if let Some(kind) = kind {
        match kind {
            MessageDialogKind::Info => MessageLevel::Info,
            MessageDialogKind::Warning => MessageLevel::Warning,
            MessageDialogKind::Error => MessageLevel::Error,
        }
    } else {
        MessageLevel::Info
    }
}

fn parse_result(cancel_label: String, result: MessageDialogResult) -> bool {
    match result {
        MessageDialogResult::Ok => true,
        MessageDialogResult::Cancel => false,
        MessageDialogResult::Yes => true,
        MessageDialogResult::No => false,
        MessageDialogResult::Custom(label) => cancel_label != label,
    }
}

pub async fn message(options: MessageDialogOptions) -> bool {
    let dialog = AsyncMessageDialog::new().set_title(options.title.as_ref().unwrap_or(&String::new())).set_level(get_level(&options.kind)).set_description(&options.message);

    let cancel_label = if let Some(cancel_id) = options.cancel_id {
        options.buttons.get(cancel_id as usize).unwrap_or(&String::new()).to_string()
    } else {
        let mut label = String::new();
        for button in &options.buttons {
            if button.eq_ignore_ascii_case(CANCEL) {
                label = button.clone();
                break;
            }

            if button.eq_ignore_ascii_case(NO) {
                label = button.clone();
                break;
            }
        }
        label
    };

    let dialog = if options.buttons.len() == 1 {
        dialog.set_buttons(MessageButtons::OkCustom(options.buttons.first().unwrap().to_string()))
    } else if options.buttons.len() == 2 {
        dialog.set_buttons(MessageButtons::OkCancelCustom(options.buttons.first().unwrap().to_string(), options.buttons.get(1).unwrap().to_string()))
    } else if options.buttons.len() == 3 {
        dialog.set_buttons(MessageButtons::YesNoCancelCustom(options.buttons.first().unwrap().to_string(), options.buttons.get(1).unwrap().to_string(), options.buttons.get(2).unwrap().to_string()))
    } else {
        dialog.set_buttons(MessageButtons::Ok)
    };

    let result = dialog.show().await;
    parse_result(cancel_label, result)
}

pub async fn open(options: OpenDialogOptions) -> FileDialogResult {
    let dialog = AsyncFileDialog::new().set_title(options.title.as_ref().unwrap_or(&String::new())).set_directory(options.default_path.as_ref().unwrap_or(&String::new()));
    let dialog = if let Some(filters) = options.filters {
        let mut dialog_result = dialog;
        for filter in filters {
            dialog_result = dialog_result.add_filter(filter.name, &filter.extensions);
        }
        dialog_result
    } else {
        dialog
    };

    if let Some(properties) = options.properties {
        if properties.contains(&OpenProperty::MultiSelections) {
            pick_multiple(dialog, properties.contains(&OpenProperty::OpenFile)).await
        } else {
            pick_single(dialog, properties.contains(&OpenProperty::OpenFile)).await
        }
    } else {
        pick_single(dialog, true).await
    }
}

async fn pick_multiple(dialog: AsyncFileDialog, pic_file: bool) -> FileDialogResult {
    let results = if pic_file {
        dialog.pick_files().await
    } else {
        dialog.pick_folders().await
    };

    if let Some(results) = results {
        let mut file_paths = Vec::new();
        for result in results {
            file_paths.push(result.path().to_string_lossy().to_string());
        }

        return FileDialogResult {
            canceled: false,
            file_paths,
        };
    }

    FileDialogResult::default()
}

async fn pick_single(dialog: AsyncFileDialog, pic_file: bool) -> FileDialogResult {
    let result = if pic_file {
        dialog.pick_file().await
    } else {
        dialog.pick_folder().await
    };

    if let Some(result) = result {
        return FileDialogResult {
            canceled: false,
            file_paths: vec![result.path().to_string_lossy().to_string()],
        };
    }

    FileDialogResult::default()
}

pub async fn save(options: SaveDialogOptions) -> FileDialogResult {
    let (directory, file_name) = if let Some(default_path) = &options.default_path {
        let path = Path::new(default_path);
        if path.is_dir() {
            (Some(path), None)
        } else {
            (Some(path.parent().unwrap_or(Path::new(""))), path.file_name().map(|s| s.to_string_lossy().to_string()))
        }
    } else {
        (None, None)
    };

    let dialog = AsyncFileDialog::new()
        .set_title(options.title.as_ref().unwrap_or(&String::new()))
        .set_directory(directory.as_ref().unwrap_or(&Path::new("")))
        .set_file_name(file_name.as_ref().unwrap_or(&String::new()));
    let dialog = if let Some(filters) = options.filters {
        let mut dialog_result = dialog;
        for filter in filters {
            dialog_result = dialog_result.add_filter(filter.name, &filter.extensions);
        }
        dialog_result
    } else {
        let extensions: Vec<String> = Vec::new();
        dialog.add_filter("All Files (*.*)", &extensions)
    };

    let result = dialog.save_file().await;

    if let Some(result) = result {
        return FileDialogResult {
            canceled: false,
            file_paths: vec![result.path().to_string_lossy().to_string()],
        };
    }

    FileDialogResult::default()
}
