#![warn(trivial_casts, trivial_numeric_casts, unused_import_braces)]
#![deny(missing_debug_implementations, missing_copy_implementations)]
#![warn(clippy::expl_impl_clone_on_copy)]
#![warn(clippy::float_cmp_const)]
#![warn(clippy::linkedlist)]
#![warn(clippy::map_flatten)]
#![warn(clippy::match_same_arms)]
#![warn(clippy::mem_forget)]
#![warn(clippy::mut_mut)]
#![warn(clippy::mutex_integer)]
#![warn(clippy::needless_continue)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::range_plus_one)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::upper_case_acronyms)]
#![cfg_attr(
    not(feature = "zopfli"),
    allow(irrefutable_let_patterns),
    allow(unreachable_patterns)
)]

#[cfg(feature = "parallel")]
extern crate rayon;

#[cfg(not(feature = "parallel"))]
mod rayon;

use std::{
    fs::{File, Metadata},
    io::{stdin, stdout, BufWriter, Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub use indexmap::{indexset, IndexSet};
use log::{debug, info, trace, warn};
use rayon::prelude::*;
pub use rgb::{RGB16, RGBA8};

pub use crate::{
    colors::{BitDepth, ColorType},
    deflate::Deflaters,
    error::PngError,
    filters::RowFilter,
    headers::StripChunks,
    interlace::Interlacing,
    options::{InFile, Options, OutFile},
};
use crate::{
    evaluate::{Candidate, Evaluator},
    headers::*,
    png::{PngData, PngImage},
    reduction::*,
};

mod apng;
mod atomicmin;
mod colors;
mod deflate;
mod display_chunks;
mod error;
mod evaluate;
mod filters;
mod headers;
mod interlace;
mod options;
mod png;
mod reduction;
#[cfg(feature = "sanity-checks")]
mod sanity_checks;

/// Private to oxipng; don't use outside tests and benches
#[doc(hidden)]
pub mod internal_tests {
    #[cfg(feature = "sanity-checks")]
    pub use crate::sanity_checks::*;
    pub use crate::{deflate::*, png::*, reduction::*};
}

pub type PngResult<T> = Result<T, PngError>;

#[derive(Debug)]
/// A raw image definition which can be used to create an optimized png
pub struct RawImage {
    png: Arc<PngImage>,
    aux_chunks: Vec<Chunk>,
}

impl RawImage {
    /// Construct a new raw image definition
    ///
    /// * `width` - The width of the image in pixels
    /// * `height` - The height of the image in pixels
    /// * `color_type` - The color type of the image
    /// * `bit_depth` - The bit depth of the image
    /// * `data` - The raw pixel data of the image
    pub fn new(
        width: u32,
        height: u32,
        color_type: ColorType,
        bit_depth: BitDepth,
        data: Vec<u8>,
    ) -> Result<Self, PngError> {
        // Validate bit depth
        let valid_depth = match color_type {
            ColorType::Grayscale { .. } => true,
            ColorType::Indexed { .. } => (bit_depth as u8) <= 8,
            _ => (bit_depth as u8) >= 8,
        };
        if !valid_depth {
            return Err(PngError::InvalidDepthForType(bit_depth, color_type));
        }

        // Validate data length
        let bpp = bit_depth as usize * color_type.channels_per_pixel() as usize;
        let row_bytes = (bpp * width as usize).div_ceil(8);
        let expected_len = row_bytes * height as usize;
        if data.len() != expected_len {
            return Err(PngError::IncorrectDataLength(data.len(), expected_len));
        }

        Ok(Self {
            png: Arc::new(PngImage {
                ihdr: IhdrData {
                    width,
                    height,
                    color_type,
                    bit_depth,
                    interlaced: Interlacing::None,
                },
                data,
            }),
            aux_chunks: Vec::new(),
        })
    }

    /// Add a png chunk, such as "iTXt", to be included in the output
    pub fn add_png_chunk(&mut self, name: [u8; 4], data: Vec<u8>) {
        self.aux_chunks.push(Chunk { name, data });
    }

    /// Add an ICC profile for the image
    pub fn add_icc_profile(&mut self, data: &[u8]) {
        // Compress with fastest compression level - will be recompressed during optimization
        let deflater = Deflaters::Libdeflater { compression: 1 };
        if let Ok(iccp) = make_iccp(data, deflater, None) {
            self.aux_chunks.push(iccp);
        }
    }

    /// Create an optimized png from the raw image data using the options provided
    pub fn create_optimized_png(&self, opts: &Options) -> PngResult<Vec<u8>> {
        let mut opts = opts.to_owned();
        let mut aux_chunks: Vec<_> = self
            .aux_chunks
            .iter()
            .filter(|c| opts.strip.keep(&c.name))
            .cloned()
            .collect();
        preprocess_chunks(&mut aux_chunks, &mut opts);

        let deadline = Arc::new(Deadline::new(opts.timeout));
        let Some(result) = optimize_raw(self.png.clone(), &opts, deadline, None) else {
            return Err(PngError::new("Failed to optimize input data"));
        };

        let mut png = PngData {
            raw: result.image,
            idat_data: result.data,
            aux_chunks,
            frames: Vec::new(),
        };
        postprocess_chunks(&mut png.aux_chunks, &png.raw.ihdr, &self.png.ihdr);

        Ok(png.output())
    }
}

/// Perform optimization on the input file using the options provided
pub fn optimize(input: &InFile, output: &OutFile, opts: &Options) -> PngResult<()> {
    // Read in the file and try to decode as PNG.
    info!("Processing: {input}");

    let deadline = Arc::new(Deadline::new(opts.timeout));

    // grab metadata before even opening input file to preserve atime
    let opt_metadata_preserved;
    let in_data = match *input {
        InFile::Path(ref input_path) => {
            if matches!(
                output,
                OutFile::Path {
                    preserve_attrs: true,
                    ..
                }
            ) {
                opt_metadata_preserved = input_path
                    .metadata()
                    .map_err(|err| {
                        // Fail if metadata cannot be preserved
                        PngError::new(&format!(
                            "Unable to read metadata from input file {input_path:?}: {err}"
                        ))
                    })
                    .map(Some)?;
                trace!("preserving metadata: {opt_metadata_preserved:?}");
            } else {
                opt_metadata_preserved = None;
            }
            PngData::read_file(input_path)?
        }
        InFile::StdIn => {
            opt_metadata_preserved = None;
            let mut data = Vec::new();
            stdin()
                .read_to_end(&mut data)
                .map_err(|e| PngError::new(&format!("Error reading stdin: {e}")))?;
            data
        }
    };

    let mut png = PngData::from_slice(&in_data, opts)?;

    // Run the optimizer on the decoded PNG.
    let mut optimized_output = optimize_png(&mut png, &in_data, opts, deadline)?;

    let in_length = in_data.len();

    if is_fully_optimized(in_data.len(), optimized_output.len(), opts) {
        match (output, input) {
            // if p is None, it also means same as the input path
            (OutFile::Path { path, .. }, InFile::Path(ref input_path))
                if path.as_ref().map_or(true, |p| p == input_path) =>
            {
                info!("{input}: Could not optimize further, no change written");
                return Ok(());
            }
            _ => {
                optimized_output = in_data;
            }
        }
    }

    let savings = if in_length >= optimized_output.len() {
        format!(
            "{} bytes ({:.2}% smaller)",
            optimized_output.len(),
            (in_length - optimized_output.len()) as f64 / in_length as f64 * 100_f64
        )
    } else {
        format!(
            "{} bytes ({:.2}% larger)",
            optimized_output.len(),
            (optimized_output.len() - in_length) as f64 / in_length as f64 * 100_f64
        )
    };

    match (output, input) {
        (OutFile::None, _) => {
            info!("{savings}: Running in pretend mode, no output");
        }
        (&OutFile::StdOut, _) | (&OutFile::Path { path: None, .. }, &InFile::StdIn) => {
            let mut buffer = BufWriter::new(stdout());
            buffer
                .write_all(&optimized_output)
                .map_err(|e| PngError::new(&format!("Unable to write to stdout: {e}")))?;
        }
        (OutFile::Path { path, .. }, _) => {
            let output_path = path
                .as_ref()
                .map(|p| p.as_path())
                .unwrap_or_else(|| input.path().unwrap());
            let out_file = File::create(output_path).map_err(|err| {
                PngError::new(&format!(
                    "Unable to write to file {}: {}",
                    output_path.display(),
                    err
                ))
            })?;
            if let Some(metadata_input) = &opt_metadata_preserved {
                copy_permissions(metadata_input, &out_file)?;
            }

            let mut buffer = BufWriter::new(out_file);
            buffer
                .write_all(&optimized_output)
                // flush BufWriter so IO errors don't get swallowed silently on close() by drop!
                .and_then(|()| buffer.flush())
                .map_err(|e| {
                    PngError::new(&format!(
                        "Unable to write to {}: {}",
                        output_path.display(),
                        e
                    ))
                })?;
            // force drop and thereby closing of file handle before modifying any timestamp
            std::mem::drop(buffer);
            if let Some(metadata_input) = &opt_metadata_preserved {
                copy_times(metadata_input, output_path)?;
            }
            info!("{}: {}", savings, output_path.display());
        }
    }
    Ok(())
}

/// Perform optimization on the input file using the options provided, where the file is already
/// loaded in-memory
pub fn optimize_from_memory(data: &[u8], opts: &Options) -> PngResult<Vec<u8>> {
    // Read in the file and try to decode as PNG.
    info!("Processing from memory");

    let deadline = Arc::new(Deadline::new(opts.timeout));

    let original_size = data.len();
    let mut png = PngData::from_slice(data, opts)?;

    // Run the optimizer on the decoded PNG.
    let optimized_output = optimize_png(&mut png, data, opts, deadline)?;

    if is_fully_optimized(original_size, optimized_output.len(), opts) {
        info!("Image already optimized");
        Ok(data.to_vec())
    } else {
        Ok(optimized_output)
    }
}

/// Perform optimization on the input PNG object using the options provided
fn optimize_png(
    png: &mut PngData,
    original_data: &[u8],
    opts: &Options,
    deadline: Arc<Deadline>,
) -> PngResult<Vec<u8>> {
    // Print png info
    let file_original_size = original_data.len();
    let idat_original_size = png.idat_data.len();
    let raw = png.raw.clone();
    debug!(
        "    {}x{} pixels, PNG format",
        raw.ihdr.width, raw.ihdr.height
    );
    report_format("    ", &raw);
    debug!("    IDAT size = {idat_original_size} bytes");
    debug!("    File size = {file_original_size} bytes");

    let mut opts = opts.to_owned();
    preprocess_chunks(&mut png.aux_chunks, &mut opts);

    let max_size = if opts.force {
        None
    } else {
        Some(png.raw.estimated_output_size(&png.idat_data))
    };
    if let Some(result) = optimize_raw(raw.clone(), &opts, deadline.clone(), max_size) {
        png.raw = result.image;
        png.idat_data = result.data;
        recompress_frames(png, &opts, deadline, result.filter)?;
        postprocess_chunks(&mut png.aux_chunks, &png.raw.ihdr, &raw.ihdr);
    }

    let output = png.output();

    if idat_original_size >= png.idat_data.len() {
        debug!(
            "    IDAT size = {} bytes ({} bytes decrease)",
            png.idat_data.len(),
            idat_original_size - png.idat_data.len()
        );
    } else {
        debug!(
            "    IDAT size = {} bytes ({} bytes increase)",
            png.idat_data.len(),
            png.idat_data.len() - idat_original_size
        );
    }
    if file_original_size >= output.len() {
        debug!(
            "    file size = {} bytes ({} bytes = {:.2}% decrease)",
            output.len(),
            file_original_size - output.len(),
            (file_original_size - output.len()) as f64 / file_original_size as f64 * 100_f64
        );
    } else {
        debug!(
            "    file size = {} bytes ({} bytes = {:.2}% increase)",
            output.len(),
            output.len() - file_original_size,
            (output.len() - file_original_size) as f64 / file_original_size as f64 * 100_f64
        );
    }

    if opts.interlace == Some(Interlacing::Adam7) && png.raw.ihdr.interlaced != Interlacing::Adam7 {
        warn!("Interlacing was not enabled as it would result in a larger file. To override this, use `--force`.");
    }

    #[cfg(feature = "sanity-checks")]
    assert!(sanity_checks::validate_output(&output, original_data));

    Ok(output)
}

/// Perform optimization on the input image data using the options provided
fn optimize_raw(
    image: Arc<PngImage>,
    opts: &Options,
    deadline: Arc<Deadline>,
    max_size: Option<usize>,
) -> Option<Candidate> {
    // Libdeflate has four algorithms: 0 = 'uncompressed', 1-4 = 'greedy', 5-7 = 'lazy', 8-9 = 'lazy2', 10-12 = 'near-optimal'
    // 5 is the minimumm required for a decent evaluation result
    // 7 is not noticeably slower than 5 and improves evaluation of filters in 'fast' mode (o2 and lower)
    // 8 is a little slower but not noticeably when used only for reductions (o3 and higher)
    // 9 is not appreciably better than 8
    // 10 and higher are quite slow - good for filters but only good for reductions if matching the main zc level
    let compression = match opts.deflate {
        Deflaters::Libdeflater { compression } => {
            if opts.fast_evaluation { 7 } else { 8 }.min(compression)
        }
        _ => 8,
    };
    let eval_deflater = Deflaters::Libdeflater { compression };
    // If only one filter is selected, use this for evaluations
    let eval_filters = if opts.filter.len() == 1 {
        opts.filter.clone()
    } else {
        // None and Bigrams work well together, especially for alpha reductions
        indexset! {RowFilter::None, RowFilter::Bigrams}
    };
    // This will collect all versions of images and pick one that compresses best
    let eval = Evaluator::new(
        deadline.clone(),
        eval_filters.clone(),
        eval_deflater,
        false,
        opts.deflate == eval_deflater,
    );
    let mut new_image = perform_reductions(image.clone(), opts, &deadline, &eval);
    let eval_result = eval.get_best_candidate();
    if let Some(ref result) = eval_result {
        new_image = result.image.clone();
    }
    let reduction_occurred = new_image.ihdr.color_type != image.ihdr.color_type
        || new_image.ihdr.bit_depth != image.ihdr.bit_depth
        || new_image.ihdr.interlaced != image.ihdr.interlaced;

    if reduction_occurred {
        report_format("Transformed image to ", &new_image);
    }

    let (result, deflater) = if opts.idat_recoding || reduction_occurred {
        let result = perform_trials(
            new_image.clone(),
            opts,
            deadline.clone(),
            max_size,
            eval_result,
            eval_filters,
            eval_deflater,
        );
        (result?, opts.deflate)
    } else {
        // If idat_recoding is off and reductions were attempted but ended up choosing the baseline,
        // we should still check if the evaluator compressed the baseline smaller than the original.
        (eval_result?, eval_deflater)
    };

    if result.data_is_compressed
        && max_size.map_or(true, |max_size| result.estimated_output_size < max_size)
    {
        debug!("Found better result:");
        debug!("    {}, f = {}", deflater, result.filter);
        return Some(result);
    }
    None
}

/// Perform compression trials
fn perform_trials(
    image: Arc<PngImage>,
    opts: &Options,
    deadline: Arc<Deadline>,
    max_size: Option<usize>,
    mut eval_result: Option<Candidate>,
    eval_filters: IndexSet<RowFilter>,
    eval_deflater: Deflaters,
) -> Option<Candidate> {
    let mut filters = opts.filter.clone();
    let fast_eval = opts.fast_evaluation && (filters.len() > 1 || eval_result.is_some());
    if fast_eval {
        // Perform a fast evaluation of selected filters followed by a single main compression trial

        if eval_result.is_some() {
            // Some filters have already been evaluated, we don't need to try them again
            filters = filters.difference(&eval_filters).copied().collect();
        }

        if !filters.is_empty() {
            trace!("Evaluating {} filters", filters.len());
            let eval = Evaluator::new(
                deadline.clone(),
                filters,
                eval_deflater,
                opts.optimize_alpha,
                opts.deflate == eval_deflater,
            );
            if let Some(result) = &eval_result {
                eval.set_best_size(result.estimated_output_size);
            }
            eval.try_image(image.clone());
            if let Some(result) = eval.get_best_candidate() {
                eval_result = Some(result);
            }
        }

        // We should have a result here - fail if not (e.g. deadline passed)
        let mut result = eval_result?;

        if !result.data_is_compressed {
            // Compress with the main deflater
            debug!("Trying filter {} with {}", result.filter, opts.deflate);
            match opts.deflate.deflate(&result.data, max_size) {
                Ok(idat_data) => {
                    result.estimated_output_size = result.image.estimated_output_size(&idat_data);
                    result.data = idat_data;
                    result.data_is_compressed = true;
                    trace!("{} bytes", result.estimated_output_size);
                }
                Err(PngError::DeflatedDataTooLong(bytes)) => {
                    trace!(">{bytes} bytes");
                }
                Err(_) => (),
            };
        }
        return Some(result);
    }

    // Perform full compression trials of selected filters and determine the best

    if filters.is_empty() {
        // Pick a filter automatically
        if image.ihdr.bit_depth as u8 >= 8 {
            // Bigrams is the best all-rounder when there's at least one byte per pixel
            filters.insert(RowFilter::Bigrams);
        } else {
            // Otherwise delta filters generally don't work well, so just stick with None
            filters.insert(RowFilter::None);
        }
    }

    debug!("Trying {} filters with {}", filters.len(), opts.deflate);
    let eval = Evaluator::new(deadline, filters, opts.deflate, opts.optimize_alpha, true);
    if let Some(max_size) = max_size {
        eval.set_best_size(max_size);
    }
    eval.try_image(image);
    eval.get_best_candidate()
}

#[derive(Debug)]
struct DeadlineImp {
    start: Instant,
    timeout: Duration,
    print_message: AtomicBool,
}

/// Keep track of processing timeout
#[doc(hidden)]
#[derive(Debug)]
pub struct Deadline {
    imp: Option<DeadlineImp>,
}

impl Deadline {
    #[must_use]
    pub fn new(timeout: Option<Duration>) -> Self {
        Self {
            imp: timeout.map(|timeout| DeadlineImp {
                start: Instant::now(),
                timeout,
                print_message: AtomicBool::new(true),
            }),
        }
    }

    /// True if the timeout has passed, and no new work should be done.
    ///
    /// If the verbose option is on, it also prints a timeout message once.
    pub fn passed(&self) -> bool {
        if let Some(imp) = &self.imp {
            let elapsed = imp.start.elapsed();
            if elapsed > imp.timeout {
                if match imp.print_message.compare_exchange(
                    true,
                    false,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(x) | Err(x) => x,
                } {
                    warn!("Timed out after {} second(s)", elapsed.as_secs());
                }
                return true;
            }
        }
        false
    }
}

/// Display the format of the image data
fn report_format(prefix: &str, png: &PngImage) {
    debug!(
        "{}{}-bit {}, {}",
        prefix, png.ihdr.bit_depth, png.ihdr.color_type, png.ihdr.interlaced
    );
}

/// Recompress the additional frames of an APNG
fn recompress_frames(
    png: &mut PngData,
    opts: &Options,
    deadline: Arc<Deadline>,
    filter: RowFilter,
) -> PngResult<()> {
    if !opts.idat_recoding || png.frames.is_empty() {
        return Ok(());
    }
    png.frames
        .par_iter_mut()
        .with_max_len(1)
        .enumerate()
        .try_for_each(|(i, frame)| {
            if deadline.passed() {
                return Ok(());
            }
            let mut ihdr = png.raw.ihdr.clone();
            ihdr.width = frame.width;
            ihdr.height = frame.height;
            let image = PngImage::new(ihdr, &frame.data)?;
            let filtered = image.filter_image(filter, opts.optimize_alpha);
            let max_size = Some(frame.data.len() - 1);
            if let Ok(data) = opts.deflate.deflate(&filtered, max_size) {
                debug!(
                    "Recompressed fdAT #{:<2}: {} ({} bytes decrease)",
                    i,
                    data.len(),
                    frame.data.len() - data.len()
                );
                frame.data = data;
            }
            Ok(())
        })
}

/// Check if an image was already optimized prior to oxipng's operations
fn is_fully_optimized(original_size: usize, optimized_size: usize, opts: &Options) -> bool {
    original_size <= optimized_size && !opts.force
}

fn copy_permissions(metadata_input: &Metadata, out_file: &File) -> PngResult<()> {
    out_file
        .set_permissions(metadata_input.permissions())
        .map_err(|err_io| {
            PngError::new(&format!(
                "unable to set permissions for output file: {err_io}"
            ))
        })
}

#[cfg(not(feature = "filetime"))]
fn copy_times(_: &Metadata, _: &Path) -> PngResult<()> {
    Ok(())
}

#[cfg(feature = "filetime")]
fn copy_times(input_path_meta: &Metadata, out_path: &Path) -> PngResult<()> {
    let atime = filetime::FileTime::from_last_access_time(input_path_meta);
    let mtime = filetime::FileTime::from_last_modification_time(input_path_meta);
    trace!("attempting to set file times: atime: {atime:?}, mtime: {mtime:?}");
    filetime::set_file_times(out_path, atime, mtime).map_err(|err_io| {
        PngError::new(&format!(
            "unable to set file times on {out_path:?}: {err_io}"
        ))
    })
}
