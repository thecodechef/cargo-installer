use std::path::PathBuf;

use clap::{value_parser, Arg, ArgAction, Command};

include!("display_chunks.rs");

pub fn build_command() -> Command {
    // Note: clap 'wrap_help' is enabled to automatically wrap lines according to terminal width.
    // To keep things tidy though, short help descriptions should be no more than 54 characters,
    // so that they can fit on a single line in an 80 character terminal.
    // Long help descriptions are soft wrapped here at 90 characters (column 91) but this does not
    // affect output, it simply matches what is rendered when help is output to a file.
    Command::new("oxipng")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Joshua Holmer <jholmer.in@gmail.com>")
        .about("Losslessly improve compression of PNG files")
        .arg(
            Arg::new("files")
                .help("File(s) to compress (use '-' for stdin)")
                .index(1)
                .num_args(1..)
                .use_value_delimiter(false)
                .required(true)
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("optimization")
                .help("Optimization level (0-6, or max)")
                .long_help("\
Set the optimization level preset. The default level 2 is quite fast and provides good \
compression. Lower levels are faster, higher levels provide better compression, though \
with increasingly diminishing returns.

    0   => --zc 5 --fast               (1 trial, determined heuristically)
    1   => --zc 10 --fast              (1 trial, determined heuristically)
    2   => --zc 11 -f 0,1,6,7 --fast   (4 fast trials, 1 main trial)
    3   => --zc 11 -f 0,7,8,9          (4 trials)
    4   => --zc 12 -f 0,7,8,9          (4 trials)
    5   => --zc 12 -f 0,1,2,5,6,7,8,9  (8 trials)
    6   => --zc 12 -f 0-9              (10 trials)
    max =>                             (stable alias for the max level)

Manually specifying a compression option (zc, f, etc.) will override the optimization \
preset, regardless of the order you write the arguments.")
                .short('o')
                .long("opt")
                .value_name("level")
                .default_value("2")
                .value_parser(["0", "1", "2", "3", "4", "5", "6", "max"])
                .hide_possible_values(true),
        )
        .arg(
            Arg::new("recursive")
                .help("Recurse input directories, optimizing all PNG files")
                .long_help("\
When directories are given as input, traverse the directory trees and optimize all PNG \
files found (files with “.png” or “.apng” extension).")
                .short('r')
                .long("recursive")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("output_dir")
                .help("Write output file(s) to <directory>")
                .long_help("\
Write output file(s) to <directory>. If the directory does not exist, it will be created. \
Note that this will not preserve the directory structure of the input files when used with \
'--recursive'.")
                .long("dir")
                .value_name("directory")
                .value_parser(value_parser!(PathBuf))
                .conflicts_with("output_file")
                .conflicts_with("stdout"),
        )
        .arg(
            Arg::new("output_file")
                .help("Write output file to <file>")
                .long("out")
                .value_name("file")
                .value_parser(value_parser!(PathBuf))
                .conflicts_with("output_dir")
                .conflicts_with("stdout"),
        )
        .arg(
            Arg::new("stdout")
                .help("Write output to stdout")
                .long("stdout")
                .action(ArgAction::SetTrue)
                .conflicts_with("output_dir")
                .conflicts_with("output_file"),
        )
        .arg(
            Arg::new("preserve")
                .help("Preserve file permissions and timestamps if possible")
                .short('p')
                .long("preserve")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pretend")
                .help("Do not write any files, only show compression results")
                .short('P')
                .long("pretend")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("strip-safe")
                .help("Strip safely-removable chunks, same as '--strip safe'")
                .short('s')
                .action(ArgAction::SetTrue)
                .conflicts_with("strip"),
        )
        .arg(
            Arg::new("strip")
                .help("Strip metadata (safe, all, or comma-separated list)\nCAUTION: 'all' will convert APNGs to standard PNGs")
                .long_help(format!("\
Strip metadata chunks, where <mode> is one of:

    safe    =>  Strip all non-critical chunks, except for the following:
                    {}
    all     =>  Strip all non-critical chunks
    <list>  =>  Strip chunks in the comma-separated list, e.g. 'bKGD,cHRM'

CAUTION: 'all' will convert APNGs to standard PNGs.

Note that 'bKGD', 'sBIT' and 'hIST' will be forcibly stripped if the color type or bit \
depth is changed, regardless of any options set.

The default when --strip is not passed is to keep all metadata.",
                       DISPLAY_CHUNKS
                           .iter()
                           .map(|c| String::from_utf8_lossy(c))
                           .collect::<Vec<_>>()
                           .join(", ")))
                .long("strip")
                .value_name("mode")
                .conflicts_with("strip-safe"),
        )
        .arg(
            Arg::new("keep")
                .help("Strip all metadata except in the comma-separated list")
                .long_help("\
Strip all metadata chunks except those in the comma-separated list. The special value \
'display' includes chunks that affect the image appearance, equivalent to '--strip safe'.

E.g. '--keep eXIf,display' will strip chunks, keeping only eXIf and those that affect the \
image appearance.")
                .long("keep")
                .value_name("list")
                .conflicts_with("strip")
                .conflicts_with("strip-safe"),
        )
        .arg(
            Arg::new("alpha")
                .help("Perform additional alpha channel optimization")
                .long_help("\
Perform additional optimization on images with an alpha channel, by altering the color \
values of fully transparent pixels. This is generally recommended for better compression, \
but take care as while this is “visually lossless”, it is technically a lossy \
transformation and may be unsuitable for some applications.")
                .short('a')
                .long("alpha")
                .action(ArgAction::SetTrue),
        )
        .arg(
            // Note: The default value is not explicitly set here, as it is dependant on the `--nx` flag.
            Arg::new("interlace")
                .help("Set PNG interlacing type (0, 1, keep) [default: 0]")
                .long_help("\
Set the PNG interlacing type, where <type> is one of:

    0     =>  Remove interlacing from all images that are processed
    1     =>  Apply Adam7 interlacing on all images that are processed
    keep  =>  Keep the existing interlacing type of each image

Note that interlacing can add 25-50% to the size of an optimized image. Only use it if you \
believe the benefits outweigh the costs for your use case.

[default: 0]")
                .short('i')
                .long("interlace")
                .value_name("type")
                .value_parser(["0", "1", "keep"])
                .hide_possible_values(true),
        )
        .arg(
            Arg::new("scale16")
                .help("Forcibly reduce 16-bit images to 8-bit (lossy)")
                .long_help("\
Forcibly reduce images with 16 bits per channel to 8 bits per channel. This is a lossy \
operation but can provide significant savings when you have no need for higher depth. \
Reduction is performed by scaling the values such that, e.g. 0x00FF is reduced to 0x01 \
rather than 0x00.

Without this flag, 16-bit images will only be reduced in depth if it can be done \
losslessly.")
                .long("scale16")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .help("Run in verbose mode (use twice to increase verbosity)")
                .short('v')
                .long("verbose")
                .action(ArgAction::Count)
                .conflicts_with("quiet"),
        )
        .arg(
            Arg::new("quiet")
                .help("Run in quiet mode")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .conflicts_with("verbose"),
        )
        .arg(
            Arg::new("filters")
                .help("Filters to try (0-9; see '--help' for details)")
                .long_help("\
Perform compression trials with each of the given filter types. You can specify a \
comma-separated list, or a range of values. E.g. '-f 0-3' is the same as '-f 0,1,2,3'.

PNG delta filters (apply the same filter to every line)
    0  =>  None      (recommended to always include this filter)
    1  =>  Sub
    2  =>  Up
    3  =>  Average
    4  =>  Paeth

Heuristic strategies (try to find the best delta filter for each line)
    5  =>  MinSum    Minimum sum of absolute differences
    6  =>  Entropy   Highest Shannon entropy
    7  =>  Bigrams   Lowest count of distinct bigrams
    8  =>  BigEnt    Highest Shannon entropy of bigrams
    9  =>  Brute     Smallest compressed size (slow)

The default value depends on the optimization level preset.")
                .short('f')
                .long("filters")
                .value_name("list"),
        )
        .arg(
            Arg::new("fast")
                .help("Use fast filter evaluation")
                .long_help("\
Perform a fast compression evaluation of each enabled filter, followed by a single main \
compression trial of the best result. Recommended if you have more filters enabled than \
CPU cores.")
                .long("fast")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("compression")
                .help("Deflate compression level (0-12)")
                .long_help("\
Deflate compression level (0-12) for main compression trials. The levels here are defined \
by the libdeflate compression library.

The default value depends on the optimization level preset.")
                .long("zc")
                .value_name("level")
                .value_parser(0..=12)
                .conflicts_with("zopfli"),
        )
        .arg(
            Arg::new("no-bit-reduction")
                .help("Do not change bit depth")
                .long("nb")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-color-reduction")
                .help("Do not change color type")
                .long("nc")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-palette-reduction")
                .help("Do not change color palette")
                .long("np")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-grayscale-reduction")
                .help("Do not change to or from grayscale")
                .long("ng")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-reductions")
                .help("Do not perform any transformations")
                .long_help("\
Do not perform any transformations and do not deinterlace by default.")
                .long("nx")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-recoding")
                .help("Do not recompress unless transformations occur")
                .long_help("\
Do not recompress IDAT unless required due to transformations. Recompression of other \
compressed chunks (such as iCCP) will also be disabled. Note that the combination of \
'--nx' and '--nz' will fully disable all optimization.")
                .long("nz")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("fix")
                .help("Disable checksum validation")
                .long_help("\
Do not perform checksum validation of PNG chunks. This may allow some files with errors to \
be processed successfully.")
                .long("fix")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force")
                .help("Write the output even if it is larger than the input")
                .long("force")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("zopfli")
                .help("Use the much slower but stronger Zopfli compressor")
                .long_help("\
Use the much slower but stronger Zopfli compressor for main compression trials. \
Recommended use is with '-o max' and '--fast'.")
                .short('Z')
                .long("zopfli")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("iterations")
                .help("Number of Zopfli iterations")
                .long_help("\
Set the number of iterations to use for Zopfli compression. Using fewer iterations may \
speed up compression for large files. This option requires '--zopfli' to be set.")
                .long("zi")
                .value_name("iterations")
                .default_value("15")
                .value_parser(1..=255)
                .requires("zopfli"),
        )
        .arg(
            Arg::new("timeout")
                .help("Maximum amount of time to spend on optimizations")
                .long_help("\
Maximum amount of time, in seconds, to spend on optimizations. Oxipng will check the \
timeout before each transformation or compression trial, and will stop trying to optimize \
the file if the timeout is exceeded. Note that this does not cut short any operations that \
are already in progress, so it is currently of limited effectiveness for large files with \
high compression levels.")
                .long("timeout")
                .value_name("secs")
                .value_parser(value_parser!(u64)),
        )
        .arg(
            Arg::new("threads")
                .help("Number of threads to use [default: num logical CPUs]")
                .long_help("\
Set the maximum number of threads to use. Oxipng uses multithreading to evaluate multiple \
optimizations on the same file in parallel as well as process multiple files in parallel. \
You can set this to a lower value if you need to limit memory or CPU usage.

[default: num logical CPUs]")
                .short('t')
                .long("threads")
                .value_name("num")
                .value_parser(value_parser!(usize)),
        )
        .arg(
            Arg::new("parallel-files")
                .help("Process multiple files sequentially")
                .long_help("\
Process multiple files sequentially rather than in parallel. Use this if you need \
determinism in the processing order. Note this is not necessary if using '--threads 1'.")
                .long("sequential")
                .action(ArgAction::SetFalse),
        )
}
