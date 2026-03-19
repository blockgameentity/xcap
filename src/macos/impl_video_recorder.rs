use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

use objc2_core_graphics::CGDirectDisplayID;
use screencapturekit::{
    cm::CMSampleBuffer,
    cv::CVPixelBufferLockFlags,
    shareable_content::SCShareableContent,
    stream::{
        configuration::{PixelFormat, SCStreamConfiguration},
        content_filter::SCContentFilter,
        output_trait::SCStreamOutputTrait,
        output_type::SCStreamOutputType,
        sc_stream::SCStream,
    },
};

use crate::{XCapError, XCapResult, video_recorder::Frame};

struct FrameHandler {
    tx: SyncSender<Frame>,
    running: Arc<AtomicBool>,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Screen {
            return;
        }

        if !self.running.load(Ordering::Acquire) {
            return;
        }

        let pixel_buffer = match sample.image_buffer() {
            Some(pb) => pb,
            None => return,
        };

        let guard = match pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
            Ok(g) => g,
            Err(_) => return,
        };

        let width = guard.width();
        let height = guard.height();
        let bytes_per_row = guard.bytes_per_row();
        let data = guard.as_slice();

        if data.is_empty() || width == 0 || height == 0 {
            return;
        }

        // Convert BGRA to RGBA, handling row padding
        let row_len = width * 4;
        let mut buffer = vec![0u8; row_len * height];

        for row_index in 0..height {
            let src_row_start = row_index * bytes_per_row;
            let dst_row_start = row_index * row_len;
            let src_row = &data[src_row_start..src_row_start + row_len];
            let dst_row = &mut buffer[dst_row_start..dst_row_start + row_len];

            for (src, dst) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
                dst[0] = src[2]; // R <- B
                dst[1] = src[1]; // G
                dst[2] = src[0]; // B <- R
                dst[3] = src[3]; // A
            }
        }

        // Check running again before sending (frames may have been queued)
        if !self.running.load(Ordering::Acquire) {
            return;
        }

        let _ = self.tx.send(Frame {
            width: width as u32,
            height: height as u32,
            raw: buffer,
        });
    }
}

#[derive(Debug)]
pub struct ImplVideoRecorder {
    stream: SCStream,
    running: Arc<AtomicBool>,
}

impl Clone for ImplVideoRecorder {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.clone(),
            running: self.running.clone(),
        }
    }
}

impl ImplVideoRecorder {
    pub fn new(cg_direct_display_id: CGDirectDisplayID) -> XCapResult<(Self, Receiver<Frame>)> {
        let content = SCShareableContent::get()
            .map_err(|e| XCapError::ScreenCaptureKit(e.to_string()))?;

        let display = content
            .displays()
            .into_iter()
            .find(|d| d.display_id() == cg_direct_display_id)
            .ok_or_else(|| {
                XCapError::new(format!(
                    "Display {} not found in SCShareableContent",
                    cg_direct_display_id
                ))
            })?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        let frame_interval = screencapturekit::cm::CMTime::new(1, 60);

        let config = SCStreamConfiguration::new()
            .with_width(display.width())
            .with_height(display.height())
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(true)
            .with_minimum_frame_interval(&frame_interval);

        let (tx, rx) = sync_channel(0);
        let running = Arc::new(AtomicBool::new(false));

        let handler = FrameHandler {
            tx,
            running: running.clone(),
        };

        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(handler, SCStreamOutputType::Screen);

        Ok((
            ImplVideoRecorder { stream, running },
            rx,
        ))
    }

    pub fn start(&self) -> XCapResult<()> {
        self.running.store(true, Ordering::Release);
        self.stream
            .start_capture()
            .map_err(|e| XCapError::ScreenCaptureKit(e.to_string()))?;
        Ok(())
    }

    pub fn stop(&self) -> XCapResult<()> {
        self.running.store(false, Ordering::Release);
        self.stream
            .stop_capture()
            .map_err(|e| XCapError::ScreenCaptureKit(e.to_string()))?;
        Ok(())
    }
}
