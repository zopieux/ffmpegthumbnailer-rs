use crate::{
    film_strip_filter, MovieDecoder, OutputContainer, OutputFormat, ThumbnailSize,
    ThumbnailerError, VideoFrame,
};

use std::{ops::Deref, path::Path};
use tokio::task::spawn_blocking;

/// `Thumbnailer` struct holds data from a `ThumbnailerBuilder`, exposing methods
/// to generate thumbnails from video files.
#[derive(Debug, Clone)]
pub struct Thumbnailer {
    builder: ThumbnailerBuilder,
}

impl Thumbnailer {
    /// Processes an video input file and outputs bytes for a specific format.
    pub async fn process_to_bytes(
        &self,
        video_file_path: impl AsRef<Path>,
        output_format: OutputFormat,
    ) -> Result<OutputContainer, ThumbnailerError> {
        let frame = self.process_to_video_frame(video_file_path).await?;
        match output_format {
            #[cfg(feature = "webp")]
            OutputFormat::Webp => self.process_to_webp_bytes(frame).await,
            #[cfg(feature = "png")]
            OutputFormat::Png => self.process_to_png_bytes(frame).await,
        }
    }

    /// Processes an video input file and write to file system a thumbnail with webp format
    #[cfg(feature = "fs")]
    pub async fn process(
        &self,
        video_file_path: impl AsRef<Path>,
        output_thumbnail_path: impl AsRef<Path>,
    ) -> Result<(), ThumbnailerError> {
        let format = match output_thumbnail_path.as_ref().extension() {
            #[cfg(feature = "webp")]
            Some(ext) if ext.eq_ignore_ascii_case("webp") => OutputFormat::Webp,
            #[cfg(feature = "png")]
            Some(ext) if ext.eq_ignore_ascii_case("png") => OutputFormat::Png,
            Some(ext) => return Err(ThumbnailerError::UnsupportedExtension(ext.to_owned())),
            None => {
                return Err(ThumbnailerError::UnsupportedExtension(
                    "<empty>".to_owned().into(),
                ))
            }
        };
        let bytes = self.process_to_bytes(video_file_path, format).await?.bytes;
        tokio::fs::write(output_thumbnail_path, bytes)
            .await
            .map_err(Into::into)
    }

    /// Processes an video input file and returns a webp encoded thumbnail as bytes
    pub async fn process_to_video_frame(
        &self,
        video_file_path: impl AsRef<Path>,
    ) -> Result<VideoFrame, ThumbnailerError> {
        let video_file_path = video_file_path.as_ref().to_path_buf();
        let prefer_embedded_metadata = self.builder.prefer_embedded_metadata;
        let seek_percentage = self.builder.seek_percentage;
        let size = self.builder.size;
        let maintain_aspect_ratio = self.builder.maintain_aspect_ratio;
        let with_film_strip = self.builder.with_film_strip;

        spawn_blocking(move || -> Result<VideoFrame, ThumbnailerError> {
            let mut decoder = MovieDecoder::new(video_file_path, prefer_embedded_metadata)?;
            // We actually have to decode a frame to get some metadata before we can start decoding for real
            decoder.decode_video_frame()?;

            if !decoder.embedded_metadata_is_available() {
                decoder.seek(
                    (decoder.get_video_duration().as_secs() as f32 * seek_percentage).round()
                        as i64,
                )?;
            }

            let mut video_frame = VideoFrame::default();

            decoder.get_scaled_video_frame(Some(size), maintain_aspect_ratio, &mut video_frame)?;

            if with_film_strip {
                film_strip_filter(&mut video_frame);
            }

            Ok(video_frame)
        })
        .await?
    }

    #[cfg(feature = "webp")]
    async fn process_to_webp_bytes(
        &self,
        video_frame: VideoFrame,
    ) -> Result<OutputContainer, ThumbnailerError> {
        let quality = self.builder.quality;
        // Type WebPMemory is !Send, which makes the Future in this function !Send,
        // this make us `deref` to have a `&[u8]` and then `to_owned` to make a Vec<u8>
        // which implies on a unwanted clone...{
        spawn_blocking(move || {
            let bytes =
                webp::Encoder::from_rgb(&video_frame.data, video_frame.width, video_frame.height)
                    .encode(quality)
                    .deref()
                    .to_vec();
            Ok(OutputContainer::from(&video_frame, bytes))
        })
        .await?
    }

    #[cfg(feature = "png")]
    async fn process_to_png_bytes(
        &self,
        video_frame: VideoFrame,
    ) -> Result<OutputContainer, ThumbnailerError> {
        spawn_blocking(move || {
            let buf: Vec<u8> = Vec::new();
            let mut writer = std::io::BufWriter::new(buf);
            let mut encoder = png::Encoder::new(&mut writer, video_frame.width, video_frame.height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()?
                .write_image_data(&video_frame.data)?;
            let bytes = writer.into_inner().unwrap();
            Ok(OutputContainer::from(&video_frame, bytes))
        })
        .await?
    }
}

/// `ThumbnailerBuilder` struct holds data to build a `Thumbnailer` struct, exposing many methods
/// to configure how a thumbnail must be generated.
#[derive(Debug, Clone)]
pub struct ThumbnailerBuilder {
    maintain_aspect_ratio: bool,
    size: ThumbnailSize,
    seek_percentage: f32,
    quality: f32,
    prefer_embedded_metadata: bool,
    with_film_strip: bool,
}

impl Default for ThumbnailerBuilder {
    fn default() -> Self {
        Self {
            maintain_aspect_ratio: true,
            size: ThumbnailSize::Size(128),
            seek_percentage: 0.1,
            quality: 80.0,
            prefer_embedded_metadata: true,
            with_film_strip: true,
        }
    }
}

impl ThumbnailerBuilder {
    /// Creates a new `ThumbnailerBuilder` with default values:
    /// - `maintain_aspect_ratio`: true
    /// - `size`: 128 pixels
    /// - `seek_percentage`: 10%
    /// - `quality`: 80
    /// - `prefer_embedded_metadata`: true
    /// - `with_film_strip`: true
    pub fn new() -> Self {
        Default::default()
    }

    /// To respect or not the aspect ratio from the video file in the generated thumbnail
    pub fn maintain_aspect_ratio(mut self, maintain_aspect_ratio: bool) -> Self {
        self.maintain_aspect_ratio = maintain_aspect_ratio;
        self
    }

    /// To set a thumbnail size, respecting or not its aspect ratio, according to `maintain_aspect_ratio` value
    pub fn size(mut self, size: u32) -> Self {
        self.size = ThumbnailSize::Size(size);
        self
    }

    /// To specify width and height of the thumbnail
    pub fn width_and_height(mut self, width: u32, height: u32) -> Self {
        self.size = ThumbnailSize::Dimensions { width, height };
        self
    }

    /// Seek percentage must be a value between 0.0 and 1.0
    pub fn seek_percentage(mut self, seek_percentage: f32) -> Result<Self, ThumbnailerError> {
        if !(0.0..=1.0).contains(&seek_percentage) {
            return Err(ThumbnailerError::InvalidSeekPercentage(seek_percentage));
        }
        self.seek_percentage = seek_percentage;
        Ok(self)
    }

    /// Quality must be a value between 0.0 and 100.0
    pub fn quality(mut self, quality: f32) -> Result<Self, ThumbnailerError> {
        if !(0.0..=100.0).contains(&quality) {
            return Err(ThumbnailerError::InvalidQuality(quality));
        }
        self.quality = quality;
        Ok(self)
    }

    /// To use embedded metadata in the video file, if available, instead of getting a frame as a
    /// thumbnail
    pub fn prefer_embedded_metadata(mut self, prefer_embedded_metadata: bool) -> Self {
        self.prefer_embedded_metadata = prefer_embedded_metadata;
        self
    }

    /// If `with_film_strip` is true, a film strip will be added to the thumbnail borders
    pub fn with_film_strip(mut self, with_film_strip: bool) -> Self {
        self.with_film_strip = with_film_strip;
        self
    }

    /// Builds a `Thumbnailer` struct
    pub fn build(self) -> Thumbnailer {
        Thumbnailer { builder: self }
    }
}
