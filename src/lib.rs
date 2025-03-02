use crate::{
    film_strip::film_strip_filter,
    movie_decoder::{MovieDecoder, ThumbnailSize},
    video_frame::VideoFrame,
};

use std::path::Path;

mod error;
mod film_strip;
mod movie_decoder;
mod thumbnailer;
mod utils;
mod video_frame;

pub use error::ThumbnailerError;
pub use thumbnailer::{Thumbnailer, ThumbnailerBuilder};

#[derive(Debug)]
pub enum OutputFormat {
    #[cfg(feature = "webp")]
    Webp,
    #[cfg(feature = "png")]
    Png,
}

#[derive(Debug)]
pub struct OutputContainer {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub bytes: Vec<u8>,
}

impl OutputContainer {
    fn from(video_frame: &VideoFrame, bytes: Vec<u8>) -> Self {
        Self {
            width: video_frame.width,
            height: video_frame.height,
            source_width: video_frame.source_width,
            source_height: video_frame.source_height,
            bytes,
        }
    }
}

/// Helper function to generate a thumbnail file from a video file with reasonable defaults
#[cfg(feature = "fs")]
pub async fn to_thumbnail(
    video_file_path: impl AsRef<Path>,
    output_thumbnail_path: impl AsRef<Path>,
    size: u32,
    quality: f32,
) -> Result<(), ThumbnailerError> {
    ThumbnailerBuilder::new()
        .size(size)
        .quality(quality)?
        .build()
        .process(video_file_path, output_thumbnail_path)
        .await
}

/// Helper function to generate a thumbnail file from a video file with reasonable defaults
pub async fn to_thumbnail_bytes(
    video_file_path: impl AsRef<Path>,
    output_format: OutputFormat,
    size: u32,
    quality: f32,
) -> Result<OutputContainer, ThumbnailerError> {
    ThumbnailerBuilder::new()
        .size(size)
        .quality(quality)?
        .build()
        .process_to_bytes(video_file_path, output_format)
        .await
}

/// Helper function to generate a thumbnail bytes from a video file with reasonable defaults
#[cfg(feature = "webp")]
pub async fn to_webp_bytes(
    video_file_path: impl AsRef<Path>,
    size: u32,
    quality: f32,
) -> Result<OutputContainer, ThumbnailerError> {
    ThumbnailerBuilder::new()
        .size(size)
        .quality(quality)?
        .build()
        .process_to_bytes(video_file_path, OutputFormat::Webp)
        .await
}

/// Helper function to generate a thumbnail bytes from a video file with reasonable defaults
#[cfg(feature = "png")]
pub async fn to_png_bytes(
    video_file_path: impl AsRef<Path>,
    size: u32,
) -> Result<OutputContainer, ThumbnailerError> {
    ThumbnailerBuilder::new()
        .size(size)
        .build()
        .process_to_bytes(video_file_path, OutputFormat::Png)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    fn get_input_filenames() -> [&'static std::path::Path; 11] {
        [
            Path::new("video_01.mp4"),
            Path::new("video_02.mov"),
            Path::new("video_03.mov"),
            Path::new("video_04.mov"),
            Path::new("video_05.mov"),
            Path::new("video_06.mov"),
            Path::new("video_07.mp4"),
            Path::new("video_08.mov"),
            Path::new("video_09.MP4"),
            Path::new("video_10.mp4"),
            Path::new("video_11.mp4"),
        ]
    }

    async fn test_all_files(format: OutputFormat) {
        let extension = match format {
            #[cfg(feature = "webp")]
            OutputFormat::Webp => "webp",
            #[cfg(feature = "png")]
            OutputFormat::Png => "png",
        };
        let input_files = get_input_filenames()
            .clone()
            .into_iter()
            .map(|p| Path::new("samples").join(p));
        let expected_output_files = get_input_filenames()
            .clone()
            .into_iter()
            .map(|p| Path::new("samples").join(p).with_extension(extension));

        let root = tempdir().unwrap();
        let actual_output_files = get_input_filenames()
            .clone()
            .into_iter()
            .map(|p| root.path().join(p).with_extension(extension));
        for (input, output) in input_files.zip(actual_output_files.clone()) {
            if let Err(e) = to_thumbnail(&input, output, 128, 100.0).await {
                eprintln!("Error: {e}; Input: {}", input.display());
                panic!("{}", e);
            }
        }

        for (expected, actual) in expected_output_files.zip(actual_output_files) {
            let expected_bytes = fs::read(expected).await.unwrap();
            let actual_bytes = fs::read(actual).await.unwrap();
            assert_eq!(expected_bytes, actual_bytes);
        }
    }

    #[tokio::test]
    #[cfg(feature = "webp")]
    async fn test_all_files_webp() {
        test_all_files(OutputFormat::Webp).await;
    }

    #[tokio::test]
    #[cfg(feature = "png")]
    async fn test_all_files_png() {
        test_all_files(OutputFormat::Png).await;
    }
}
