use anyhow::Result;
use bisection::bisect_left;
use clap::Parser;
use elementtree::Element;
use log::{debug, error};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::str::FromStr;
#[macro_use]
extern crate lazy_static;
extern crate ndarray;
use ndarray::Array2;
use rayon::prelude::*;
extern crate reqwest;
use tempfile::tempdir;
use url::Url;

mod apsp;
use apsp::{shortest_path, Algorithm, INFINITY};

mod pdf;
use pdf::generate_pdfs;

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Algorithm for All Pairs Shortest Paths
    #[clap(arg_enum, short = 'a', long, default_value = "dial")]
    algorithm: Algorithm,

    /// Minimum BTN to use in route calculations
    #[clap(short = 'b', long, default_value = DEFAULT_MIN_BTN)]
    min_btn: f64,

    /// Directory where we read and write data files
    #[clap(short = 'd', long)]
    data_directory: Option<PathBuf>,

    /// Path to a file containing one sector name per line.  Multiples are allowed
    #[clap(short = 'f', long, multiple_occurrences = true)]
    file_of_sectors: Vec<PathBuf>,

    // GTFT18 says to only use jump-2 routes except for xboat routes and
    // when jump-3 can save a feeder or better route at least one jump.
    /// Default maximum jump
    #[clap(short = 'j', long, default_value = "2")]
    max_jump: u8,

    /// Maximum jump for minor routes
    #[clap(short = '1', long, default_value = "2")]
    max_jump_minor: u8,

    /// Maximum jump for feeder routes
    #[clap(short = '2', long, default_value = "3")]
    max_jump_feeder: u8,

    /// Maximum jump for intermediate routes
    #[clap(short = '3', long, default_value = "3")]
    max_jump_intermediate: u8,

    /// Maximum jump for main routes
    #[clap(short = '4', long, default_value = "3")]
    max_jump_main: u8,

    /// Maximum jump for major routes
    #[clap(short = '5', long, default_value = "3")]
    max_jump_major: u8,

    /// Directory where we place output PDFs
    #[clap(short = 'o', long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    /// No output
    #[clap(short = 'q', long)]
    quiet: bool,

    /// Minimum BTN to draw a route on the map
    #[clap(short = 'r', long, default_value = DEFAULT_MIN_ROUTE_BTN)]
    min_route_btn: f64,

    /// Name of a sector to process.  Multiples are allowed.
    #[clap(short = 's', long, multiple_occurrences = true)]
    sector: Vec<String>,

    /// Level of verbosity.  Repeat for more output.
    #[clap(short = 'v', long, parse(from_occurrences))]
    verbose: usize,

    /// Ignore xboat routes; don't let them ignore max_jump
    #[clap(short = 'X', long)]
    ignore_xboat_routes: bool,

    /// Use Passenger BTN instead of Freight BTN
    #[clap(short = 'p', long)]
    passenger: bool,
}

const MAX_TECH_LEVEL: u32 = 23;
const MAX_POPULATION: u32 = 15;

const MAX_DISTANCE_PENALTY: f64 = 9999.0;

// Rules don't say BTN can't be negative but it seems reasonable to me.
const ABSOLUTE_MIN_BTN: f64 = 0.0;
const MAX_BTN_WTN_DELTA: f64 = 5.0;

const RI_PBTN_BONUS: f64 = 0.5;
const CP_PBTN_BONUS: f64 = 0.5;
const CS_PBTN_BONUS: f64 = 0.5;

const AG_WTCM_BONUS: f64 = 0.5;
const IN_WTCM_BONUS: f64 = 0.5;
const MAX_WTCM_BONUS: f64 = AG_WTCM_BONUS + IN_WTCM_BONUS;
const DIFFERENT_ALLEGIANCE_WTCM_PENALTY: f64 = 0.5;
const MAX_WTCM_PENALTY: f64 = DIFFERENT_ALLEGIANCE_WTCM_PENALTY;

const DEFAULT_MIN_BTN: &str = "6.5";
const DEFAULT_MIN_ROUTE_BTN: &str = "8.0";

const NON_IMPERIAL_PORT_SIZE_PENALTY: f64 = 0.5;
const NEIGHBOR_1_PORT_SIZE_BONUS: f64 = 1.5;
const NEIGHBOR_2_PORT_SIZE_BONUS: f64 = 1.0;
const XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE: f64 = 6.0;
const FEEDER_ROUTE_MIN_PORT_SIZE: f64 = 5.0;
const MINOR_ROUTE_MIN_PORT_SIZE: f64 = 4.0;

lazy_static! {
    static ref STARPORT_TRAVELLER_TO_GURPS: HashMap<char, String> = {
        let mut sttg: HashMap<char, String> = HashMap::new();
        sttg.insert('A', "V".to_string());
        sttg.insert('B', "IV".to_string());
        sttg.insert('C', "III".to_string());
        sttg.insert('D', "II".to_string());
        sttg.insert('E', "I".to_string());
        sttg.insert('X', "0".to_string());
        sttg
    };

    static ref TECH_LEVEL_TRAVELLER_TO_GURPS: HashMap<u32, u64> = {
        let mut tttg: HashMap<u32, u64> = HashMap::new();
        tttg.insert(0, 2); // actually 1-3
        tttg.insert(1, 4);
        tttg.insert(2, 5);
        tttg.insert(3, 5);
        tttg.insert(4, 5);
        tttg.insert(5, 6);
        tttg.insert(6, 6);
        tttg.insert(7, 7);
        tttg.insert(8, 8);
        tttg.insert(9, 9);
        tttg.insert(10, 9);
        tttg.insert(11, 9);
        tttg.insert(12, 10);
        tttg.insert(13, 10);
        tttg.insert(14, 11);
        tttg.insert(15, 12);
        tttg.insert(16, 13);
        tttg.insert(17, 13);
        tttg.insert(18, 14);
        tttg.insert(19, 14);
        tttg.insert(20, 14);
        tttg.insert(21, 14);
        tttg.insert(22, 14);
        tttg.insert(23, 14);
        tttg
    };

    static ref WTN_PORT_MODIFIER_TABLE: HashMap<(u64, String), f64> = {
        let mut wpmt: HashMap<(u64, String), f64> = HashMap::new();
        wpmt.insert((7, "V".to_string()), 0.0);
        wpmt.insert((7, "IV".to_string()), -1.0);
        wpmt.insert((7, "III".to_string()), -1.5);
        wpmt.insert((7, "II".to_string()), -2.0);
        wpmt.insert((7, "I".to_string()), -2.5);
        wpmt.insert((7, "0".to_string()), -5.0);
        wpmt.insert((6, "V".to_string()), 0.0);
        wpmt.insert((6, "IV".to_string()), -0.5);
        wpmt.insert((6, "III".to_string()), -1.0);
        wpmt.insert((6, "II".to_string()), -1.5);
        wpmt.insert((6, "I".to_string()), -2.0);
        wpmt.insert((6, "0".to_string()), -4.5);
        wpmt.insert((5, "V".to_string()), 0.0);
        wpmt.insert((5, "IV".to_string()), 0.0);
        wpmt.insert((5, "III".to_string()), -0.5);
        wpmt.insert((5, "II".to_string()), -1.0);
        wpmt.insert((5, "I".to_string()), -1.5);
        wpmt.insert((5, "0".to_string()), -4.0);
        wpmt.insert((4, "V".to_string()), 0.5);
        wpmt.insert((4, "IV".to_string()), 0.0);
        wpmt.insert((4, "III".to_string()), 0.0);
        wpmt.insert((4, "II".to_string()), -0.5);
        wpmt.insert((4, "I".to_string()), -1.0);
        wpmt.insert((4, "0".to_string()), -3.5);
        wpmt.insert((3, "V".to_string()), 0.5);
        wpmt.insert((3, "IV".to_string()), 0.5);
        wpmt.insert((3, "III".to_string()), 0.0);
        wpmt.insert((3, "II".to_string()), 0.0);
        wpmt.insert((3, "I".to_string()), -0.5);
        wpmt.insert((3, "0".to_string()), -3.0);
        wpmt.insert((2, "V".to_string()), 1.0);
        wpmt.insert((2, "IV".to_string()), 0.5);
        wpmt.insert((2, "III".to_string()), 0.5);
        wpmt.insert((2, "II".to_string()), 0.0);
        wpmt.insert((2, "I".to_string()), 0.0);
        wpmt.insert((2, "0".to_string()), -2.5);
        wpmt.insert((1, "V".to_string()), 1.0);
        wpmt.insert((1, "IV".to_string()), 1.0);
        wpmt.insert((1, "III".to_string()), 0.5);
        wpmt.insert((1, "II".to_string()), 0.0);
        wpmt.insert((1, "I".to_string()), 0.0);
        wpmt.insert((1, "0".to_string()), 0.0);
        wpmt.insert((0, "V".to_string()), 1.5);
        wpmt.insert((0, "IV".to_string()), 1.0);
        wpmt.insert((0, "III".to_string()), 1.0);
        wpmt.insert((0, "II".to_string()), 0.5);
        wpmt.insert((0, "I".to_string()), 0.5);
        wpmt.insert((0, "0".to_string()), 0.0);
        wpmt
    };

    // Values on GTFT16 are ranges, but we use averages for repeatability.
    static ref DBTN_TO_CREDITS: Vec<u64> = vec![
        0,  // GTFT16 says 0-5, but 0 DBTN can also mean unreachable, so use 0.
        7,
        30,
        75,
        300,
        750,
        3000,
        7500,
        30000,
        75000,
        300000,
        750000,
        3000000,
        7500000,
        30000000,
        75000000,
        300000000,
        750000000,
        3000000000,
        75000000000,
        300000000000,
        750000000000,
        3000000000000,
        7500000000000,
        30000000000000,
        75000000000000,
        300000000000000,
    ];

    static ref MIN_BTN: f64 = f64::from_str(DEFAULT_MIN_BTN).unwrap();
    static ref MIN_ROUTE_BTN: f64 = f64::from_str(DEFAULT_MIN_ROUTE_BTN).unwrap();
}

fn download_sector_data(data_dir: &Path, sector_names: &Vec<String>) -> Result<()> {
    debug!("download_sector_data");
    for sector_name in sector_names {
        let sector_data_filename = sector_name.to_owned() + ".sec";
        let mut data_path = data_dir.to_path_buf();
        data_path.push(sector_data_filename);
        let sector_xml_filename = sector_name.to_owned() + ".xml";
        let mut metadata_path = data_dir.to_path_buf();
        metadata_path.push(sector_xml_filename);
        let base_url = Url::parse("https://travellermap.com/data/")?;
        if !data_path.exists() {
            let data_url = base_url.join(sector_name)?;
            debug!("downloading {}", data_url);
            let body = reqwest::blocking::get(data_url)?.text()?;
            write(data_path, body)?;
        }
        if !metadata_path.exists() {
            let metadata_url = base_url.join(&(sector_name.to_owned() + "/metadata"))?;
            debug!("downloading {}", metadata_url);
            let body = reqwest::blocking::get(metadata_url)?.text()?;
            write(metadata_path, body)?;
        }
    }
    Ok(())
}

/// Parse header and separator and return [(start, end, field)]
fn parse_header_and_separator(header: &str, separator: &str) -> Vec<(usize, usize, String)> {
    let headers: Vec<&str> = header.split_whitespace().collect();
    let separators = separator.split_whitespace();
    let mut fields: Vec<(usize, usize, String)> = Vec::new();
    let mut column = 0;
    for (ii, hyphens) in separators.enumerate() {
        let field = headers[ii];
        let start = column;
        let width = hyphens.len();
        let end = column + width;
        fields.push((start, end, field.to_string()));
        column += width + 1;
    }
    fields
}

/// Find minimum distances between all worlds, and predecessor paths.
/// Only use jumps of up to max_jump hexes, except along xboat routes
/// if ignore_xboat_routes is not set.
/// Must be run after all neighbors are built.
fn populate_navigable_distances(
    sorted_coords: &Vec<Coords>,
    coords_to_world: &HashMap<Coords, World>,
    max_jump: u8,
    ignore_xboat_routes: bool,
    alg: Algorithm,
) -> (Array2<u16>, Array2<u16>) {
    debug!("populate_navigable_distances max_jump={}", max_jump);
    let num_worlds = sorted_coords.len();
    if num_worlds >= u16::MAX as usize {
        error!("Too many worlds for a u16!  We will overflow!");
        exit(3);
    }
    let mut np = Array2::<u16>::zeros((num_worlds, num_worlds));
    let mut num_edges = 0;
    for (ii, coords) in sorted_coords.iter().enumerate() {
        let world = coords_to_world.get(coords).unwrap();
        for jump in 1..=max_jump {
            for coords in &world.neighbors[jump as usize] {
                let neighbor = coords_to_world.get(coords).unwrap();
                let jj = neighbor.index.unwrap();
                np[[ii, jj]] = jump as u16;
                num_edges += 1;
            }
        }
        if !ignore_xboat_routes {
            for coords in &world.xboat_routes {
                let neighbor = coords_to_world.get(coords).unwrap();
                let jj = neighbor.index.unwrap();
                np[[ii, jj]] = world.straight_line_distance(neighbor) as u16;
                num_edges += 1;
            }
        }
    }
    debug!(
        "(parallel) shortest_path alg={:?} worlds={} edges={}",
        alg, num_worlds, num_edges
    );
    let pred = shortest_path(&mut np, alg);
    (np, pred)
}

fn distance_modifier_table(distance: u16) -> f64 {
    if distance == INFINITY {
        return MAX_DISTANCE_PENALTY;
    }
    let table: Vec<u16> = vec![1, 2, 5, 9, 19, 29, 59, 99, 199, 299, 599, 999, INFINITY];
    let index = bisect_left(&table, &distance);
    index as f64 / 2.0
}

fn same_allegiance(allegiance1: &str, allegiance2: &str) -> bool {
    if allegiance1 != allegiance2 {
        return false;
    }
    if allegiance1.starts_with("Na") || allegiance1.starts_with("Cs") {
        // Non-aligned worlds and client states with the same code are not
        // necessarily the same allegiance
        return false;
    }
    true
}

fn find_max_allowed_jump(credits: u64, max_jumps: &[u8], min_route_btn: f64) -> u8 {
    let feeder_route_threshold: f64 = min_route_btn + 1.0;
    let intermediate_route_threshold: f64 = min_route_btn + 2.0;
    let main_route_threshold: f64 = min_route_btn + 3.0;
    let major_route_threshold: f64 = min_route_btn + 4.0;
    let trade_dbtn = bisect_left(&DBTN_TO_CREDITS, &credits);
    let trade_btn = trade_dbtn as f64 / 2.0;
    if trade_btn >= major_route_threshold {
        return max_jumps[5];
    } else if trade_btn >= main_route_threshold {
        return max_jumps[4];
    } else if trade_btn >= intermediate_route_threshold {
        return max_jumps[3];
    } else if trade_btn >= feeder_route_threshold {
        return max_jumps[2];
    }
    max_jumps[1]
}

/// Fill in major_routes, main_routes, intermediate_routes, minor_routes,
/// and feeder_routes for all Worlds.
///
/// This must be called after all Sectors and Worlds are mostly built.
/// The rules say: main: 10+  feeder: 9-9.5  minor: 8-8.5
/// The wiki says: blue major 12, cyan main 11, green intermediate 10,
///                yellow feeder 9, red minor 8, no line 1-7
/// The wiki version is more fun so we'll use that.
fn populate_trade_routes(
    coords_to_world: &mut HashMap<Coords, World>,
    coords_to_index: &HashMap<Coords, usize>,
    sorted_coords: &[Coords],
    min_btn: f64,
    min_route_btn: f64,
    passenger: bool,
    max_jumps: &[u8],
    dists: &HashMap<u8, Array2<u16>>,
    preds: &HashMap<u8, Array2<u16>>,
) {
    debug!("populate_trade_routes");
    let mut dwtn_coords: Vec<(u64, Coords)> = Vec::new();
    for (coords, world) in coords_to_world.iter() {
        // wtn can have 0.5 so double it to make a sortable integer
        let dwtn = (world.wtn() * 2.0) as u64;
        dwtn_coords.push((dwtn, *coords));
    }
    dwtn_coords.sort();
    dwtn_coords.reverse();

    debug!("Building world trade pairs");
    // Add endpoint trade credits to both endpoints
    let mut coords_pairs: Vec<CoordsPair> = Vec::new();
    for (ii, (dwtn1, coords1)) in dwtn_coords.iter().enumerate() {
        let wtn1 = *dwtn1 as f64 / 2.0;
        for (dwtn2, coords2) in dwtn_coords.iter().skip(ii + 1) {
            let wtn2 = *dwtn2 as f64 / 2.0;
            if wtn2 < min_btn - MAX_BTN_WTN_DELTA || wtn1 + wtn2 < min_btn - MAX_WTCM_BONUS {
                // If the lower WTN or the sum of the WTNs is small enough, we
                // know that coords2 and later worlds won't come close to
                // forming any trade routes with coords1.
                break;
            }
            let sld = coords1.straight_line_distance(coords2) as u16;
            let max_btn1 = wtn1 + wtn2 - distance_modifier_table(sld);
            if max_btn1 < min_btn - MAX_WTCM_BONUS {
                // BTN can't be more than the sum of the WTNs plus the bonus,
                // so if even the straight line distance modifier puts us too
                // low, we can't come close to forming any trade routes with
                // world2.
                continue;
            }
            let world1 = coords_to_world.get(coords1).unwrap();
            let world2 = coords_to_world.get(coords2).unwrap();
            if max_btn1 < min_btn + MAX_WTCM_PENALTY {
                // Computing the wtcm is cheaper than finding the full BTN
                let wtcm = world1.wtcm(world2);
                let max_btn2 = max_btn1 + wtcm;
                if max_btn2 < min_btn {
                    continue;
                }
            }
            // At this point we have exhausted ways to skip world2 without
            // computing the BTN.
            coords_pairs.push((*coords1, *coords2));
        }
    }

    let max_max_jump: u8 = *max_jumps.iter().max().unwrap();

    debug!("(parallel) Finding BTNs");
    // This will consider all jumps, even those only allowed for higher routes.
    // So we need to filter some out later.
    let dist = dists.get(&max_max_jump).unwrap();
    let coords_pair_dbtn_credits: Vec<(Coords, Coords, usize, u64)> = coords_pairs
        .into_par_iter()
        .map(|(coords1, coords2)| {
            let world1 = coords_to_world.get(&coords1).unwrap();
            let world2 = coords_to_world.get(&coords2).unwrap();
            let btn = world1.btn(world2, dist, passenger);
            let dbtn = (2.0 * btn) as usize;
            let credits = DBTN_TO_CREDITS[dbtn];
            (coords1, coords2, dbtn, credits)
        })
        .collect();

    debug!("Recording BTNs");
    for (coords1, coords2, dbtn, credits) in coords_pair_dbtn_credits {
        coords_to_world
            .get_mut(&coords1)
            .unwrap()
            .endpoint_trade_credits += credits;
        coords_to_world
            .get_mut(&coords2)
            .unwrap()
            .endpoint_trade_credits += credits;

        coords_to_world.get_mut(&coords1).unwrap().dbtn_to_coords[dbtn].insert(coords2);
        coords_to_world.get_mut(&coords2).unwrap().dbtn_to_coords[dbtn].insert(coords1);
    }

    debug!("(parallel) Finding route paths");

    let result_tuples: Vec<(HashMap<CoordsPair, u64>, HashMap<Coords, u64>)> = dwtn_coords
        .into_par_iter()
        .map(|(_, coords)| {
            coords_to_world.get(&coords).unwrap().find_route_paths(
                sorted_coords,
                coords_to_world,
                coords_to_index,
                max_jumps,
                min_route_btn,
                dists,
                preds,
            )
        })
        .collect();
    let mut route_paths: HashMap<CoordsPair, u64> = HashMap::new();
    let mut coords_to_transient_credits: HashMap<Coords, u64> = HashMap::new();
    for (rp, cttc) in result_tuples {
        for (coord_tup, credits) in rp {
            route_paths
                .entry(coord_tup)
                .and_modify(|count| *count += credits)
                .or_insert(credits);
        }
        for (coords, credits) in cttc {
            coords_to_transient_credits
                .entry(coords)
                .and_modify(|count| *count += credits)
                .or_insert(credits);
        }
    }

    debug!("Inserting trade routes");
    let minor_route_threshold: f64 = min_route_btn;
    let feeder_route_threshold: f64 = min_route_btn + 1.0;
    let intermediate_route_threshold: f64 = min_route_btn + 2.0;
    let main_route_threshold: f64 = min_route_btn + 3.0;
    let major_route_threshold: f64 = min_route_btn + 4.0;

    for ((coords1, coords2), credits) in route_paths {
        let trade_dbtn = bisect_left(&DBTN_TO_CREDITS, &credits);
        let trade_btn = trade_dbtn as f64 / 2.0;
        if trade_btn >= major_route_threshold {
            coords_to_world
                .get_mut(&coords1)
                .unwrap()
                .major_routes
                .insert(coords2);
            coords_to_world
                .get_mut(&coords2)
                .unwrap()
                .major_routes
                .insert(coords1);
        } else if trade_btn >= main_route_threshold {
            coords_to_world
                .get_mut(&coords1)
                .unwrap()
                .main_routes
                .insert(coords2);
            coords_to_world
                .get_mut(&coords2)
                .unwrap()
                .main_routes
                .insert(coords1);
        } else if trade_btn >= intermediate_route_threshold {
            coords_to_world
                .get_mut(&coords1)
                .unwrap()
                .intermediate_routes
                .insert(coords2);
            coords_to_world
                .get_mut(&coords2)
                .unwrap()
                .intermediate_routes
                .insert(coords1);
        } else if trade_btn >= feeder_route_threshold {
            coords_to_world
                .get_mut(&coords1)
                .unwrap()
                .feeder_routes
                .insert(coords2);
            coords_to_world
                .get_mut(&coords2)
                .unwrap()
                .feeder_routes
                .insert(coords1);
        } else if trade_btn >= minor_route_threshold {
            coords_to_world
                .get_mut(&coords1)
                .unwrap()
                .minor_routes
                .insert(coords2);
            coords_to_world
                .get_mut(&coords2)
                .unwrap()
                .minor_routes
                .insert(coords1);
        }
    }

    debug!("Updating transient credits");
    for (coords, credits) in coords_to_transient_credits {
        coords_to_world
            .get_mut(&coords)
            .unwrap()
            .transient_trade_credits += credits;
    }
}

/// Absolute coordinates
/// x is an integer
/// y2 is an integer, equal to 2 * y
/// This is needed because y is sometimes a float and floats can't be hash keys
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Coords {
    x: i64,
    y2: i64,
}

impl Coords {
    fn new(xf: f64, yf: f64) -> Coords {
        let x = xf as i64;
        let y2 = (yf * 2.0) as i64;
        Coords { x, y2 }
    }

    fn straight_line_distance(&self, other: &Coords) -> u16 {
        let (x1, y1) = <(f64, f64)>::from(*self);
        let (x2, y2) = <(f64, f64)>::from(*other);
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        (f64::floor(xdelta + ydelta)) as u16
    }
}

impl From<Coords> for (f64, f64) {
    fn from(coords: Coords) -> (f64, f64) {
        let fx = coords.x as f64;
        let fy = coords.y2 as f64 / 2.0;
        (fx, fy)
    }
}

type CoordsPair = (Coords, Coords);

#[derive(Clone, Debug, Eq)]
pub struct World {
    sector_location: (i64, i64),
    hex: String,
    name: String,
    uwp: String,
    trade_classifications: HashSet<String>,
    importance: i64,
    economic: String,
    cultural: String,
    nobles: String,
    bases: HashSet<String>,
    zone: char,
    pbg: String,
    worlds: u64,
    allegiance: String,
    stars: Vec<String>,
    endpoint_trade_credits: u64,
    transient_trade_credits: u64,
    xboat_routes: HashSet<Coords>,
    dbtn_to_coords: Vec<HashSet<Coords>>,
    major_routes: HashSet<Coords>,
    main_routes: HashSet<Coords>,
    intermediate_routes: HashSet<Coords>,
    feeder_routes: HashSet<Coords>,
    minor_routes: HashSet<Coords>,
    neighbors: Vec<HashSet<Coords>>,
    index: Option<usize>,
}

impl World {
    fn new(line: String, fields: &[(usize, usize, String)], sector_location: (i64, i64)) -> World {
        let mut hex = "".to_string();
        let mut name = "".to_string();
        let mut uwp = "".to_string();
        let mut trade_classifications = HashSet::new();
        let mut importance = 0;
        let mut economic = "".to_string();
        let mut cultural = "".to_string();
        let mut nobles = "".to_string();
        let mut bases = HashSet::new();
        let mut zone = 'G';
        let mut pbg = "".to_string();
        let mut worlds = 0;
        let mut allegiance = "".to_string();
        let mut stars = Vec::new();
        let endpoint_trade_credits = 0;
        let transient_trade_credits = 0;
        let xboat_routes = HashSet::new();
        let mut dbtn_to_coords = Vec::new();
        // Pre-populate every dbtn bucket with an empty set so we don't need
        // to deal with checking later.
        for _ in 0..DBTN_TO_CREDITS.len() {
            dbtn_to_coords.push(HashSet::new());
        }
        let major_routes = HashSet::new();
        let main_routes = HashSet::new();
        let intermediate_routes = HashSet::new();
        let feeder_routes = HashSet::new();
        let minor_routes = HashSet::new();
        let neighbors = Vec::new();
        let index = None;

        let mut iter = line.chars().enumerate();
        for (start, end, field) in fields.iter() {
            // This intricate loop is to handle the occasional multi-byte
            // UTF-8 character like in Khiinra Ash/Core
            let mut value: String = "".to_string();
            loop {
                let tup_opt: Option<(usize, char)> = iter.next();
                if let Some((ii, ch)) = tup_opt {
                    if ii >= *start && ii < *end {
                        value.push(ch);
                    } else if ii >= *end {
                        break;
                    }
                } else {
                    // end of line
                    break;
                }
            }

            match field.as_str() {
                "Hex" => hex = value.to_string(),
                "Name" => name = value.trim().to_string(),
                "UWP" => uwp = value.to_string(),
                "Remarks" => {
                    for tc in value.trim().split_whitespace() {
                        trade_classifications.insert(tc.to_string());
                    }
                }
                "{Ix}" => {
                    let trimmed = value
                        .trim_matches(|c| c == '{' || c == '}' || c == ' ')
                        .to_string();
                    if !trimmed.is_empty() {
                        if let Ok(val) = trimmed.parse() {
                            importance = val;
                        }
                    }
                }
                "(Ex)" => economic = value.trim_matches(|c| c == '(' || c == ')').to_string(),
                "[Cx]" => cultural = value.trim_matches(|c| c == '[' || c == ']').to_string(),
                "N" => nobles = value.trim_matches(|c| c == ' ' || c == '-').to_string(),
                "B" => {
                    let trimmed = value.trim_matches(|c| c == ' ' || c == '-').to_string();
                    if !trimmed.is_empty() {
                        for ch in trimmed.chars() {
                            bases.insert(ch.to_string());
                        }
                    }
                }
                "Z" => {
                    let trimmed = value.trim_matches(|c| c == ' ' || c == '-').to_string();
                    if !trimmed.is_empty() {
                        zone = trimmed.chars().next().unwrap();
                    }
                }
                "PBG" => pbg = value.trim().to_string(),
                "W" => {
                    let trimmed = value
                        .trim_matches(|c| c == '{' || c == '}' || c == ' ')
                        .to_string();
                    if !trimmed.is_empty() {
                        if let Ok(val) = trimmed.parse() {
                            worlds = val;
                        }
                    }
                }
                "A" => allegiance = value.to_string(),
                "Stellar" => {
                    let parts: Vec<&str> = value.trim().split_whitespace().collect();
                    let mut ii = 0;
                    while ii < parts.len() {
                        let star = parts[ii];
                        if star == "BD" || star == "D" || star == "BH" {
                            stars.push(star.to_owned());
                            ii += 1;
                        } else if parts.len() > ii + 1 {
                            stars.push(star.to_owned() + " " + parts[ii + 1]);
                            ii += 2;
                        } else {
                            stars.push(star.to_owned());
                            ii += 1;
                        }
                    }
                }
                &_ => (),
            }
        }

        World {
            sector_location,
            hex,
            name,
            uwp,
            trade_classifications,
            importance,
            economic,
            cultural,
            nobles,
            bases,
            zone,
            pbg,
            worlds,
            allegiance,
            stars,
            endpoint_trade_credits,
            transient_trade_credits,
            xboat_routes,
            dbtn_to_coords,
            major_routes,
            main_routes,
            intermediate_routes,
            feeder_routes,
            minor_routes,
            neighbors,
            index,
        }
    }

    /// Find and cache all neighbors within 3 hexes.
    ///
    /// This must be run after all Sectors and Worlds are mostly initialized.
    fn populate_neighbors(&mut self, coords_to_world: &HashMap<Coords, World>, max_jump: u8) {
        // The 0 index is unused, but fill it in anyway to make the other
        // indexes nicer.
        for _jump in 0..=max_jump {
            self.neighbors.push(HashSet::new());
        }
        if !self.can_refuel() {
            return;
        }
        let (x, y) = <(f64, f64)>::from(self.get_coords());
        let mut xx = x - max_jump as f64;
        while xx <= x + max_jump as f64 {
            let mut yy = y - max_jump as f64;
            while yy <= y + max_jump as f64 {
                let world_opt = coords_to_world.get(&Coords::new(xx, yy));
                if let Some(world) = world_opt {
                    if world != self && world.can_refuel() {
                        let distance = self.straight_line_distance(world);
                        if distance <= max_jump as u16 {
                            self.neighbors[distance as usize].insert(world.get_coords());
                        }
                    }
                }
                yy += 0.5;
            }
            xx += 1.0;
        }
    }

    fn starport(&self) -> char {
        return self.uwp.chars().next().unwrap() as char;
    }

    fn g_starport(&self) -> String {
        let mut starport = self.starport();
        if starport == '?' {
            starport = 'X';
        }
        let opt = STARPORT_TRAVELLER_TO_GURPS.get(&starport);
        opt.unwrap().to_string()
    }

    fn size(&self) -> char {
        return self.uwp.chars().nth(1).unwrap() as char;
    }

    fn atmosphere(&self) -> char {
        return self.uwp.chars().nth(2).unwrap() as char;
    }

    fn hydrosphere(&self) -> char {
        return self.uwp.chars().nth(3).unwrap() as char;
    }

    fn population(&self) -> char {
        return self.uwp.chars().nth(4).unwrap() as char;
    }

    fn government(&self) -> char {
        return self.uwp.chars().nth(5).unwrap() as char;
    }

    fn law_level(&self) -> char {
        return self.uwp.chars().nth(6).unwrap() as char;
    }

    fn tech_level(&self) -> char {
        return self.uwp.chars().nth(8).unwrap() as char;
    }

    fn g_tech_level(&self) -> u64 {
        let mut tech_level_char = self.tech_level();
        if tech_level_char == '?' || tech_level_char == 'X' {
            tech_level_char = '0';
        }
        let tech_level_int = tech_level_char.to_digit(MAX_TECH_LEVEL + 1).unwrap();
        *TECH_LEVEL_TRAVELLER_TO_GURPS.get(&tech_level_int).unwrap()
    }

    fn gas_giants(&self) -> char {
        self.pbg.chars().nth(2).unwrap()
    }

    fn can_refuel(&self) -> bool {
        self.gas_giants() != '0'
            || (self.zone != 'R'
                && ((self.starport() != 'E' && self.starport() != 'X')
                    || self.hydrosphere() != '0'))
    }

    fn uwtn(&self) -> f64 {
        let gt3 = self.g_tech_level() / 3;
        let tl_mod = gt3 as f64 / 2.0 - 0.5;
        let pop_char = self.population();
        let mut pop_mod = 0.0;
        if pop_char.is_alphanumeric() && pop_char != 'X' {
            // ignore '?'
            let pop_int = pop_char.to_digit(MAX_POPULATION + 1).unwrap();
            pop_mod = pop_int as f64 / 2.0;
        }
        tl_mod + pop_mod as f64
    }

    fn wtn_port_modifier(&self) -> f64 {
        let iuwtn = u64::max(0, self.uwtn() as u64);
        *WTN_PORT_MODIFIER_TABLE
            .get(&(iuwtn, self.g_starport()))
            .unwrap()
    }

    fn wtn(&self) -> f64 {
        self.uwtn() + self.wtn_port_modifier()
    }

    fn wtcm(&self, other: &World) -> f64 {
        let mut result = 0.0;

        if (self.trade_classifications.contains("Ag")
            && (other.trade_classifications.contains("Ex")
                || other.trade_classifications.contains("Na")))
            || (other.trade_classifications.contains("Ag")
                && (self.trade_classifications.contains("Ex")
                    || self.trade_classifications.contains("Na")))
        {
            result += AG_WTCM_BONUS;
        }

        if (self.trade_classifications.contains("In") && other.trade_classifications.contains("Ni"))
            || (other.trade_classifications.contains("In")
                && self.trade_classifications.contains("Ni"))
        {
            result += IN_WTCM_BONUS;
        }
        if !same_allegiance(&self.allegiance, &other.allegiance) {
            result -= DIFFERENT_ALLEGIANCE_WTCM_PENALTY;
        }
        result
    }

    fn get_coords(&self) -> Coords {
        let hex = &self.hex;
        let location = self.sector_location;
        let mut scratch = String::new();
        scratch.push(hex.chars().next().unwrap());
        scratch.push(hex.chars().nth(1).unwrap());
        let x: i64 = scratch.parse::<i64>().unwrap() + 32 * location.0;
        scratch.clear();
        scratch.push(hex.chars().nth(2).unwrap());
        scratch.push(hex.chars().nth(3).unwrap());
        let y: i64 = scratch.parse::<i64>().unwrap() + 40 * location.1;
        let mut y2 = 2 * y;
        if x & 1 == 0 {
            y2 += 1;
        }
        Coords { x, y2 }
    }

    fn straight_line_distance(&self, other: &World) -> u16 {
        let (x1, y1) = <(f64, f64)>::from(self.get_coords());
        let (x2, y2) = <(f64, f64)>::from(other.get_coords());
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        (f64::floor(xdelta + ydelta)) as u16
    }

    fn navigable_distance(&self, other: &World, dist: &Array2<u16>) -> u16 {
        let ii = self.index.unwrap();
        let jj = other.index.unwrap();
        dist[[ii, jj]]
    }

    /// Return the inclusive path from self to other.
    fn navigable_path(
        &self,
        other: &World,
        sorted_coords: &[Coords],
        coords_to_index: &HashMap<Coords, usize>,
        dist: &Array2<u16>,
        pred: &Array2<u16>,
    ) -> Option<Vec<Coords>> {
        if self == other {
            return Some(vec![self.get_coords()]);
        }
        if self.navigable_distance(other, dist) == INFINITY {
            return None;
        }
        let mut path = vec![self.get_coords()];
        let mut coords2 = self.get_coords();
        loop {
            let ii = other.index.unwrap();
            let jj = coords_to_index.get(&coords2).unwrap();
            let index = pred[[ii, *jj]];
            coords2 = sorted_coords[index as usize];
            if coords2 == other.get_coords() {
                path.push(coords2);
                break;
            } else {
                path.push(coords2);
            }
        }
        Some(path)
    }

    fn distance_modifier(&self, other: &World, dist: &Array2<u16>) -> f64 {
        let distance = self.navigable_distance(other, dist);
        distance_modifier_table(distance)
    }

    fn btn(&self, other: &World, dist: &Array2<u16>, passenger: bool) -> f64 {
        let wtn1 = self.wtn();
        let wtn2 = other.wtn();
        let min_wtn = f64::min(wtn1, wtn2);
        let base_btn = wtn1 + wtn2 + self.wtcm(other);
        let mut btn = base_btn - self.distance_modifier(other, dist);
        if passenger {
            for world in [self, other] {
                if world.trade_classifications.contains("Ri") {
                    btn += RI_PBTN_BONUS;
                }
                if world.trade_classifications.contains("Cp") {
                    btn += CP_PBTN_BONUS;
                }
                if world.trade_classifications.contains("Cs") {
                    btn += CS_PBTN_BONUS;
                }
            }
        }
        f64::max(ABSOLUTE_MIN_BTN, f64::min(btn, min_wtn + MAX_BTN_WTN_DELTA))
    }

    /// Build a map of CoordsPairs to the credits of trade between them, and a
    /// map of Coords to its total transient (non-endpoint) trade credits.
    fn find_route_paths(
        &self,
        sorted_coords: &[Coords],
        coords_to_world: &HashMap<Coords, World>,
        coords_to_index: &HashMap<Coords, usize>,
        max_jumps: &[u8],
        min_route_btn: f64,
        dists: &HashMap<u8, Array2<u16>>,
        preds: &HashMap<u8, Array2<u16>>,
    ) -> (HashMap<CoordsPair, u64>, HashMap<Coords, u64>) {
        let mut route_paths: HashMap<CoordsPair, u64> = HashMap::new();
        let mut coords_to_transient_credits: HashMap<Coords, u64> = HashMap::new();
        let all_jumps_set: HashSet<u8> = max_jumps.iter().cloned().collect();
        let mut all_jumps: Vec<u8> = all_jumps_set.iter().cloned().collect();
        all_jumps.sort_unstable();
        for (dbtn, coords_set) in self.dbtn_to_coords.iter().enumerate() {
            let credits = DBTN_TO_CREDITS[dbtn];
            let max_allowed_jump = find_max_allowed_jump(credits, max_jumps, min_route_btn);
            for coords2 in coords_set {
                let world2 = coords_to_world.get(coords2).unwrap();
                let mut path: Vec<Coords> = Vec::new();
                for jump in all_jumps.iter() {
                    // Only allow jumps that are allowed based on the route size.
                    if jump <= &max_allowed_jump {
                        let dist = dists.get(jump).unwrap();
                        let pred = preds.get(jump).unwrap();
                        let possible_path_opt =
                            self.navigable_path(world2, sorted_coords, coords_to_index, dist, pred);
                        if let Some(possible_path) = possible_path_opt {
                            // Only use bigger jumps if that saves us a hop.
                            if path.is_empty() || possible_path.len() < path.len() {
                                path = possible_path;
                            }
                        }
                    }
                }
                if path.len() >= 2 {
                    for ii in 0..path.len() - 1 {
                        let first = path.get(ii).unwrap();
                        let second = path.get(ii + 1).unwrap();
                        let coord_tup: CoordsPair = if first <= second {
                            (*first, *second)
                        } else {
                            (*second, *first)
                        };
                        route_paths
                            .entry(coord_tup)
                            .and_modify(|count| *count += credits)
                            .or_insert(credits);
                    }
                    for jj in 1..path.len() - 2 {
                        let coords3 = path.get(jj).unwrap();
                        coords_to_transient_credits
                            .entry(*coords3)
                            .and_modify(|transient| *transient += credits)
                            .or_insert(credits);
                    }
                }
            }
        }
        (route_paths, coords_to_transient_credits)
    }

    fn imperial_affiliated(&self) -> bool {
        self.allegiance == "CsIm" || self.allegiance.starts_with("Im")
    }

    // This only works after trade routes are built.
    fn port_size(&self) -> u64 {
        let mut port_size = self.wtn();
        if !self.imperial_affiliated() {
            port_size -= NON_IMPERIAL_PORT_SIZE_PENALTY;
        }
        if !self.neighbors[1].is_empty() {
            port_size += NEIGHBOR_1_PORT_SIZE_BONUS;
        } else if self.neighbors.len() > 2 && !self.neighbors[2].is_empty() {
            port_size += NEIGHBOR_2_PORT_SIZE_BONUS;
        }
        port_size = f64::ceil(port_size);
        if !self.xboat_routes.is_empty() || !self.major_routes.is_empty() {
            if port_size < XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE {
                port_size = XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE;
            }
        } else if !self.main_routes.is_empty()
            || !self.intermediate_routes.is_empty()
            || !self.feeder_routes.is_empty()
        {
            if port_size < FEEDER_ROUTE_MIN_PORT_SIZE {
                port_size = FEEDER_ROUTE_MIN_PORT_SIZE;
            }
        } else if !self.minor_routes.is_empty() && port_size < MINOR_ROUTE_MIN_PORT_SIZE {
            port_size = MINOR_ROUTE_MIN_PORT_SIZE;
        }
        port_size as u64
    }
}

impl PartialEq for World {
    fn eq(&self, other: &Self) -> bool {
        self.hex == other.hex && self.name == other.name
    }
}

impl Hash for World {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hex.hash(state);
        self.name.hash(state);
    }
}

impl Ord for World {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_coords().cmp(&other.get_coords())
    }
}

impl PartialOrd for World {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_coords().partial_cmp(&other.get_coords())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sector {
    name: String,
    names: Vec<String>,
    abbreviation: String,
    location: (i64, i64),
    subsector_letter_to_name: HashMap<char, String>,
    allegiance_code_to_name: HashMap<String, String>,
    hex_to_coords: HashMap<String, Coords>,
}

impl Sector {
    fn new(
        data_dir: &Path,
        sector_name: String,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Sector {
        let name = sector_name.to_owned();
        let names = Vec::new();
        let abbreviation = "".to_string();
        let location = (-1, -1);
        let subsector_letter_to_name = HashMap::new();
        let allegiance_code_to_name = HashMap::new();
        let hex_to_coords = HashMap::new();
        let mut sector = Sector {
            name,
            names,
            abbreviation,
            location,
            subsector_letter_to_name,
            allegiance_code_to_name,
            hex_to_coords,
        };

        sector.parse_xml_metadata(data_dir, &sector_name).unwrap();
        sector
            .parse_column_data(data_dir, &sector_name, coords_to_world)
            .unwrap();
        sector
    }

    fn parse_xml_metadata(&mut self, data_dir: &Path, sector_name: &str) -> Result<()> {
        let mut xml_path = data_dir.to_path_buf();
        xml_path.push(sector_name.to_owned() + ".xml");
        let xml_file = File::open(xml_path)?;
        let root = Element::from_reader(xml_file)?;

        let abbreviation_opt = root.get_attr("Abbreviation");
        if let Some(abbreviation) = abbreviation_opt {
            self.abbreviation = abbreviation.to_string();
        }

        let mut x = i64::MAX;
        let x_opt = root.find("X");
        if let Some(x_element) = x_opt {
            let x_text = x_element.text();
            x = x_text.parse()?;
        }
        let mut y = i64::MAX;
        let y_opt = root.find("Y");
        if let Some(y_element) = y_opt {
            let y_text = y_element.text();
            y = y_text.parse()?;
        }
        self.location = (x, y);

        let name_elements = root.find_all("Name");
        for name_element in name_elements {
            if !name_element.text().is_empty() {
                self.names.push(name_element.text().to_string());
            }
        }

        let subsectors_opt = root.find("Subsectors");
        if let Some(subsectors_element) = subsectors_opt {
            let subsector_elements = subsectors_element.find_all("Subsector");
            for subsector_element in subsector_elements {
                let index_opt = subsector_element.get_attr("Index");
                if let Some(index) = index_opt {
                    let letter = index.chars().next().unwrap();
                    let subsector_name = subsector_element.text().to_string();
                    if !subsector_name.is_empty() {
                        self.subsector_letter_to_name.insert(letter, subsector_name);
                    }
                }
            }
        }

        let allegiances_opt = root.find("Allegiances");
        if let Some(allegiances_element) = allegiances_opt {
            let allegiance_elements = allegiances_element.find_all("Allegiance");
            for allegiance_element in allegiance_elements {
                let code_opt = allegiance_element.get_attr("Code");
                if let Some(code) = code_opt {
                    let allegiance_name = allegiance_element.text().to_string();
                    if !allegiance_name.is_empty() {
                        self.allegiance_code_to_name
                            .insert(code.to_string(), allegiance_name);
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_column_data(
        &mut self,
        data_dir: &Path,
        sector_name: &str,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Result<()> {
        let mut data_path = data_dir.to_path_buf();
        data_path.push(sector_name.to_owned() + ".sec");
        let blob = read_to_string(data_path)?;
        let mut header = "";
        // We initialize fields here to make rustc happy, then overwrite it.
        let mut fields: Vec<(usize, usize, String)> = Vec::new();
        for line in blob.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("Hex") {
                header = line;
            } else if line.starts_with("---") {
                let separator = line;
                fields = parse_header_and_separator(header, separator);
            } else {
                let world = World::new(line.to_string(), &fields, self.location);
                self.hex_to_coords
                    .insert(world.hex.clone(), world.get_coords());
                coords_to_world.insert(world.get_coords(), world);
            }
        }

        Ok(())
    }

    /// Parse Xboat routes from xml
    /// Must be called after all Sectors and Worlds are built
    fn parse_xml_routes(
        &self,
        data_dir: &Path,
        location_to_sector: &HashMap<(i64, i64), Sector>,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Result<()> {
        let mut xml_path = data_dir.to_path_buf();
        xml_path.push(self.name.to_owned() + ".xml");
        let xml_file = File::open(xml_path)?;
        let root = Element::from_reader(xml_file)?;
        let routes_opt = root.find("Routes");
        if let Some(routes_element) = routes_opt {
            let route_elements = routes_element.find_all("Route");
            for route_element in route_elements {
                let start_hex_opt = route_element.get_attr("Start");
                if let Some(start_hex) = start_hex_opt {
                    let end_hex_opt = route_element.get_attr("End");
                    if let Some(end_hex) = end_hex_opt {
                        let start_offset_x_opt = route_element.get_attr("StartOffsetX");
                        let mut start_offset_x = 0;
                        if let Some(start_offset_x2) = start_offset_x_opt {
                            start_offset_x = start_offset_x2.parse()?;
                        };
                        let start_offset_y_opt = route_element.get_attr("StartOffsetY");
                        let mut start_offset_y = 0;
                        if let Some(start_offset_y2) = start_offset_y_opt {
                            start_offset_y = start_offset_y2.parse()?;
                        }
                        let end_offset_x_opt = route_element.get_attr("EndOffsetX");
                        let mut end_offset_x = 0;
                        if let Some(end_offset_x2) = end_offset_x_opt {
                            end_offset_x = end_offset_x2.parse()?;
                        }
                        let end_offset_y_opt = route_element.get_attr("EndOffsetY");
                        let mut end_offset_y = 0;
                        if let Some(end_offset_y2) = end_offset_y_opt {
                            end_offset_y = end_offset_y2.parse()?;
                        }
                        let start_sector_opt = location_to_sector.get(&(
                            self.location.0 + start_offset_x,
                            self.location.1 + start_offset_y,
                        ));
                        let end_sector_opt = location_to_sector.get(&(
                            self.location.0 + end_offset_x,
                            self.location.1 + end_offset_y,
                        ));
                        if let Some(start_sector) = start_sector_opt {
                            if let Some(end_sector) = end_sector_opt {
                                if let Some(start_coords) =
                                    start_sector.hex_to_coords.get(start_hex)
                                {
                                    if let Some(end_coords) = end_sector.hex_to_coords.get(end_hex)
                                    {
                                        // Need to do these one at a time to avoid holding two
                                        // mutable references at once.
                                        if let Some(_start_world) =
                                            coords_to_world.get(start_coords)
                                        {
                                            if let Some(end_world) =
                                                coords_to_world.get_mut(end_coords)
                                            {
                                                end_world.xboat_routes.insert(*start_coords);
                                            }
                                        }
                                        if let Some(_end_world) = coords_to_world.get(end_coords) {
                                            if let Some(start_world) =
                                                coords_to_world.get_mut(start_coords)
                                            {
                                                start_world.xboat_routes.insert(*end_coords);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn hex_to_world<'a>(
        &'a self,
        hex: String,
        coords_to_world: &'a HashMap<Coords, World>,
    ) -> Option<&World> {
        let coords_opt = self.hex_to_coords.get(&hex);
        if let Some(coords) = coords_opt {
            return coords_to_world.get(coords);
        }
        None
    }
}

fn parse_file_of_sectors(file_of_sectors: PathBuf) -> Result<HashSet<String>> {
    let mut sector_names: HashSet<String> = HashSet::new();
    let mut file = File::open(file_of_sectors)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    for line in buf.lines() {
        sector_names.insert(line.trim().to_string());
    }

    Ok(sector_names)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let verbose = args.verbose;
    let quiet = args.quiet;
    if quiet && verbose > 0 {
        eprintln!("Please do not set both --quiet and --verbose.  Exiting");
        exit(1);
    }
    let alg = args.algorithm;

    let output_dir = args.output_directory;
    let temp_dir = tempdir()?;
    let mut data_dir: PathBuf = temp_dir.path().to_path_buf();
    if let Some(data_dir_override) = args.data_directory {
        data_dir = data_dir_override;
    };
    let mut sector_names_set: HashSet<String> = HashSet::new();
    for sector_name in args.sector {
        sector_names_set.insert(sector_name);
    }
    for filename in args.file_of_sectors {
        if let Ok(sector_names_set2) = parse_file_of_sectors(filename) {
            for sector_name in sector_names_set2 {
                sector_names_set.insert(sector_name);
            }
        }
    }
    let mut sector_names: Vec<String> = sector_names_set.into_iter().collect();
    sector_names.sort();

    let ignore_xboat_routes = args.ignore_xboat_routes;
    let min_btn = args.min_btn;
    let min_route_btn = args.min_route_btn;
    let passenger = args.passenger;
    let max_jumps: Vec<u8> = vec![
        args.max_jump,
        args.max_jump_minor,
        args.max_jump_feeder,
        args.max_jump_intermediate,
        args.max_jump_main,
        args.max_jump_major,
    ];
    let max_max_jump: u8 = *max_jumps.iter().max().unwrap();

    stderrlog::new()
        .module(module_path!())
        .quiet(quiet)
        .verbosity(verbose)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .init()
        .unwrap();

    if sector_names.is_empty() {
        error!("No sectors.  Exiting.");
        exit(2);
    }
    debug!("{} sectors: {:?}", sector_names.len(), sector_names);

    create_dir_all(&output_dir)?;
    create_dir_all(&data_dir)?;

    download_sector_data(&data_dir, &sector_names)?;

    debug!("Building sectors");
    let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
    let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
    for sector_name in sector_names {
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);
        location_to_sector.insert(sector.location, sector);
    }
    debug!("Building routes and neighbors");
    for sector in location_to_sector.values() {
        sector
            .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
            .unwrap();
    }
    {
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2, max_max_jump);
        }
    }
    let mut sorted_coords: Vec<Coords> = coords_to_world.keys().cloned().collect();
    sorted_coords.sort();
    let mut coords_to_index: HashMap<Coords, usize> = HashMap::new();
    for (ii, coords) in sorted_coords.iter_mut().enumerate() {
        coords_to_index.insert(*coords, ii);
        let world = coords_to_world.get_mut(coords).unwrap();
        world.index = Some(ii);
    }

    let all_jumps: HashSet<u8> = max_jumps.iter().cloned().collect();
    let mut dists: HashMap<u8, Array2<u16>> = HashMap::new();
    let mut preds: HashMap<u8, Array2<u16>> = HashMap::new();
    for jump in all_jumps.iter() {
        let (dist, pred) = populate_navigable_distances(
            &sorted_coords,
            &coords_to_world,
            *jump,
            ignore_xboat_routes,
            alg,
        );
        dists.insert(*jump, dist);
        preds.insert(*jump, pred);
    }

    populate_trade_routes(
        &mut coords_to_world,
        &coords_to_index,
        &sorted_coords,
        min_btn,
        min_route_btn,
        passenger,
        &max_jumps,
        &dists,
        &preds,
    );

    generate_pdfs(&output_dir, &location_to_sector, &coords_to_world);

    temp_dir.close()?;

    debug!("Exit");

    Ok(())
}
