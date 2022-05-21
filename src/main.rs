use anyhow::Result;
use bisection::bisect_left;
extern crate cairo;
use cairo::{Context, FontFace, FontSlant, FontWeight, PdfSurface};
use clap::Parser;
use elementtree::Element;
use log::{debug, error};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::process::exit;
#[macro_use]
extern crate lazy_static;
extern crate ndarray;
use ndarray::Array2;
extern crate rand;
use rand::{random, thread_rng, Rng};
use rayon::prelude::*;
extern crate reqwest;
use tempfile::tempdir;
use url::Url;

mod apsp;
use apsp::{dijkstra, INFINITY};
#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of a sector to process.  Multiples are allowed.
    #[clap(short, long, multiple_occurrences = true)]
    sector: Vec<String>,

    /// Path to a file containing one sector name per line
    #[clap(short, long, multiple_occurrences = true)]
    file_of_sectors: Vec<PathBuf>,

    /// Directory where we read and write data files
    #[clap(short, long)]
    data_directory: Option<PathBuf>,

    /// Directory where we place output PDFs
    #[clap(short, long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    /// Level of verbosity.  Repeat for more output.
    #[clap(short, long, parse(from_occurrences))]
    verbose: usize,

    /// No output
    #[clap(short, long)]
    quiet: bool,
}

const SQRT3: f64 = 1.7320508075688772;

const MAX_TECH_LEVEL: u32 = 34;
const MAX_POPULATION: u32 = 34;

// Rules don't say BTN can't be negative but it seems reasonable to me.
const MIN_BTN: f64 = 0.0;
const MAX_BTN_WTN_DELTA: f64 = 5.0;

const RI_PBTN_BONUS: f64 = 0.5;
const CP_PBTN_BONUS: f64 = 0.5;
const CS_PBTN_BONUS: f64 = 0.5;

const AG_WTCM_BONUS: f64 = 0.5;
const IN_WTCM_BONUS: f64 = 0.5;
const MAX_WTCM_BONUS: f64 = AG_WTCM_BONUS + IN_WTCM_BONUS;
const DIFFERENT_ALLEGIANCE_WTCM_PENALTY: f64 = 0.5;
const MAX_WTCM_PENALTY: f64 = DIFFERENT_ALLEGIANCE_WTCM_PENALTY;

const MAJOR_ROUTE_THRESHOLD: f64 = 12.0;
const MAIN_ROUTE_THRESHOLD: f64 = 11.0;
const INTERMEDIATE_ROUTE_THRESHOLD: f64 = 10.0;
const FEEDER_ROUTE_THRESHOLD: f64 = 9.0;
const MINOR_ROUTE_THRESHOLD: f64 = 8.0;
const TRIVIAL_ROUTE_THRESHOLD: f64 = 6.5;

const NON_IMPERIAL_PORT_SIZE_PENALTY: f64 = 0.5;
const NEIGHBOR_1_PORT_SIZE_BONUS: f64 = 1.5;
const NEIGHBOR_2_PORT_SIZE_BONUS: f64 = 1.0;
const XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE: f64 = 6.0;
const FEEDER_ROUTE_MIN_PORT_SIZE: f64 = 5.0;
const MINOR_ROUTE_MIN_PORT_SIZE: f64 = 4.0;

const MIN_DBTN_FOR_JUMP_3: usize = (2.0 * FEEDER_ROUTE_THRESHOLD) as usize;

const SCALE: f64 = 15.0;
const SECTOR_HEX_WIDTH: i64 = 32;
const SECTOR_HEX_HEIGHT: i64 = 40;

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
        tttg.insert(24, 14);
        tttg.insert(25, 14);
        tttg.insert(26, 14);
        tttg.insert(27, 14);
        tttg.insert(28, 14);
        tttg.insert(29, 14);
        tttg.insert(30, 14);
        tttg.insert(31, 14);
        tttg.insert(32, 14);
        tttg.insert(33, 14);
        tttg.insert(34, 14);
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

    // Values in the book are ranges, but use averages for repeatability.
    static ref DBTN_TO_CREDITS: Vec<u64> = vec![
        2,
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
}

fn download_sector_data(data_dir: &PathBuf, sector_names: &Vec<String>) -> Result<()> {
    debug!("download_sector_data");
    for sector_name in sector_names {
        let sector_data_filename = sector_name.to_owned() + ".sec";
        let mut data_path = data_dir.clone();
        data_path.push(sector_data_filename);
        let sector_xml_filename = sector_name.to_owned() + ".xml";
        let mut metadata_path = data_dir.clone();
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
    return fields;
}

/// Find minimum distances between all worlds, and predecessor paths.
/// Only use jumps of up to max_jump hexes, except along xboat routes.
/// Must be run after all neighbors are built.
fn populate_navigable_distances(
    sorted_coords: &Vec<Coords>,
    coords_to_world: &HashMap<Coords, World>,
    max_jump: u64,
) -> (Array2<i32>, Array2<i32>) {
    debug!("populate_navigable_distances max_jump={}", max_jump);
    let num_worlds = sorted_coords.len();
    let mut np = Array2::<i32>::zeros((num_worlds, num_worlds));
    let mut num_edges = 0;
    for (ii, coords) in sorted_coords.iter().enumerate() {
        let world = coords_to_world.get(coords).unwrap();
        if max_jump >= 3 {
            for coords in &world.neighbors3 {
                let neighbor = coords_to_world.get(&coords).unwrap();
                let jj = neighbor.index.unwrap();
                np[[ii, jj]] = 3;
                num_edges += 1;
            }
        }
        if max_jump >= 2 {
            for coords in &world.neighbors2 {
                let neighbor = coords_to_world.get(&coords).unwrap();
                let jj = neighbor.index.unwrap();
                np[[ii, jj]] = 2;
                num_edges += 1;
            }
        }
        if max_jump >= 1 {
            for coords in &world.neighbors1 {
                let neighbor = coords_to_world.get(&coords).unwrap();
                let jj = neighbor.index.unwrap();
                np[[ii, jj]] = 1;
                num_edges += 1;
            }
        }
        for coords in &world.xboat_routes {
            let neighbor = coords_to_world.get(&coords).unwrap();
            let jj = neighbor.index.unwrap();
            np[[ii, jj]] = world.straight_line_distance(neighbor) as i32;
            num_edges += 1;
        }
    }
    debug!(
        "(parallel) dijkstra worlds={} edges={}",
        num_worlds, num_edges
    );
    let pred = dijkstra(&mut np);
    return (np, pred);
}

fn distance_modifier_table(distance: i32) -> f64 {
    let table: Vec<i32> = vec![1, 2, 5, 9, 19, 29, 59, 99, 199, 299, 599, 999, INFINITY];
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
    sorted_coords: &Vec<Coords>,
    dist2: &Array2<i32>,
    pred2: &Array2<i32>,
    dist3: &Array2<i32>,
    pred3: &Array2<i32>,
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
    let mut coords_pairs: Vec<(Coords, Coords)> = Vec::new();
    for (ii, (dwtn1, coords1)) in dwtn_coords.iter().enumerate() {
        let wtn1 = *dwtn1 as f64 / 2.0;
        for jj in ii + 1..dwtn_coords.len() {
            let (dwtn2, coords2) = dwtn_coords[jj];
            let wtn2 = dwtn2 as f64 / 2.0;
            if wtn2 < TRIVIAL_ROUTE_THRESHOLD - MAX_BTN_WTN_DELTA
                || wtn1 + wtn2 < TRIVIAL_ROUTE_THRESHOLD - MAX_WTCM_BONUS
            {
                // BTN can't be more than the lower WTN + 5, or the sum of the
                // WTNs plus 1.  So if the lower WTN or the sum of the the sum
                // of the WTNs is small enough, we know that coords2 and later
                // worlds won't come close to forming any trade routes with
                // coords1.
                break;
            }
            let sld = coords1.straight_line_distance(&coords2) as i32;
            let max_btn1 = wtn1 + wtn2 - distance_modifier_table(sld);
            if max_btn1 < TRIVIAL_ROUTE_THRESHOLD - MAX_WTCM_BONUS {
                // BTN can't be more than the sum of the WTNs plus the bonus,
                // so if even the straight line distance modifier puts us too
                // low, we can't come close to forming any trade routes with
                // world2.
                continue;
            }
            let world1 = coords_to_world.get(&coords1).unwrap();
            let world2 = coords_to_world.get(&coords2).unwrap();
            if max_btn1 < TRIVIAL_ROUTE_THRESHOLD + MAX_WTCM_PENALTY {
                // Computing the wtcm is cheaper than finding the full BTN
                let wtcm = world1.wtcm(&world2);
                let max_btn2 = max_btn1 + wtcm;
                if max_btn2 < TRIVIAL_ROUTE_THRESHOLD {
                    continue;
                }
            }
            // At this point we have exhausted ways to skip world2 without
            // computing the BTN.
            coords_pairs.push((*coords1, coords2));
        }
    }

    debug!("(parallel) Finding BTNs");
    let coords_coords_dbtn_credits: Vec<(Coords, Coords, usize, u64)> = coords_pairs
        .into_par_iter()
        .map(|(coords1, coords2)| {
            let world1 = coords_to_world.get(&coords1).unwrap();
            let world2 = coords_to_world.get(&coords2).unwrap();
            let btn = world1.btn(&world2, dist2);
            let dbtn = (2.0 * btn) as usize;
            let credits = DBTN_TO_CREDITS[dbtn];
            (coords1, coords2, dbtn, credits)
        })
        .collect();

    debug!("Recording BTNs");
    for (coords1, coords2, dbtn, credits) in coords_coords_dbtn_credits {
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

    let result_tuples: Vec<(HashMap<(Coords, Coords), u64>, HashMap<Coords, u64>)>;
    result_tuples = dwtn_coords
        .into_par_iter()
        .map(|(_, coords)| {
            coords_to_world.get(&coords).unwrap().find_route_paths(
                &sorted_coords,
                &coords_to_world,
                &coords_to_index,
                &dist2,
                &pred2,
                &dist3,
                &pred3,
            )
        })
        .collect();
    let mut route_paths: HashMap<(Coords, Coords), u64> = HashMap::new();
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
    for ((coords1, coords2), credits) in route_paths {
        let trade_dbtn = bisect_left(&DBTN_TO_CREDITS, &credits);
        let trade_btn = trade_dbtn as f64 / 2.0;
        if trade_btn >= MAJOR_ROUTE_THRESHOLD {
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
        } else if trade_btn >= MAIN_ROUTE_THRESHOLD {
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
        } else if trade_btn >= INTERMEDIATE_ROUTE_THRESHOLD {
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
        } else if trade_btn >= FEEDER_ROUTE_THRESHOLD {
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
        } else if trade_btn >= MINOR_ROUTE_THRESHOLD {
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

fn draw_neighboring_sector_name(
    ctx: &Context,
    font_face: &FontFace,
    name: &str,
    x_pos: f64,
    y_pos: f64,
) {
    // TODO Vertical text on the left and right sides would save space
    ctx.set_font_size(SCALE);
    ctx.set_font_face(font_face);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
    let extents = ctx.text_extents(name).unwrap();
    ctx.move_to(x_pos - extents.width / 2.0, y_pos - extents.height / 2.0);
    ctx.show_text(name).unwrap();
}

fn init_vars(
    sector: &Sector,
    x: i64,
    y: i64,
) -> (
    String,
    f64,
    f64,
    Vec<(f64, f64)>,
    (f64, f64),
    Option<&Coords>,
) {
    let hex = format!("{:02}{:02}", x, y);
    // leftmost point
    let cx = (4.0 + x as f64) * 3.0 * SCALE;
    // topmost point
    let cy = (3.0 + y as f64 * 2.0 + ((x as f64 - 1.0) as i64 & 1) as f64) * SQRT3 * SCALE;
    let mut vertexes: Vec<(f64, f64)> = Vec::new();
    vertexes.push((cx + SCALE, cy));
    vertexes.push((cx + 3.0 * SCALE, cy));
    vertexes.push((cx + 4.0 * SCALE, cy + SQRT3 * SCALE));
    vertexes.push((cx + 3.0 * SCALE, cy + 2.0 * SQRT3 * SCALE));
    vertexes.push((cx + SCALE, cy + 2.0 * SQRT3 * SCALE));
    vertexes.push((cx, cy + SQRT3 * SCALE));
    let center = (cx + 2.0 * SCALE, cy + SQRT3 * SCALE);
    let coords_opt = sector.hex_to_coords.get(&hex);
    return (hex, cx, cy, vertexes, center, coords_opt);
}

fn draw_route(
    ctx: &Context,
    coords1: Coords,
    coords_set: &HashSet<Coords>,
    line_width: f64,
    rgba: (f64, f64, f64, f64),
    cx: f64,
    cy: f64,
    center: (f64, f64),
) {
    let (x1, y1) = <(f64, f64)>::from(coords1);
    for coords2 in coords_set.iter() {
        let (x2, y2) = <(f64, f64)>::from(*coords2);
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;
        let cx2 = cx + delta_x * 3.0 * SCALE;
        let cy2 = cy + delta_y * 2.0 * SQRT3 * SCALE;
        let center2 = (cx2 + 2.0 * SCALE, cy2 + SQRT3 * SCALE);
        ctx.set_line_width(line_width);
        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        ctx.move_to(center.0, center.1);
        ctx.line_to(center2.0, center2.1);
        ctx.stroke().unwrap();
    }
}

fn generate_pdf(
    sector: &Sector,
    output_dir: &PathBuf,
    location_to_sector: &HashMap<(i64, i64), Sector>,
    coords_to_world: &HashMap<Coords, World>,
) {
    let width = 60.0 * SECTOR_HEX_WIDTH as f64 * SCALE;
    let height = 35.0 * SQRT3 * SECTOR_HEX_HEIGHT as f64 * SCALE;
    let output_filename = sector.name.to_owned() + ".pdf";
    let mut output_path = output_dir.clone();
    output_path.push(output_filename);

    let surface = PdfSurface::new(width, height, output_path).unwrap();
    let ctx = Context::new(&surface).unwrap();
    ctx.scale(SCALE, SCALE);

    // background
    ctx.set_source_rgba(0.0, 0.0, 0.0, 1.0); // black
    ctx.rectangle(0.0, 0.0, width, height);
    ctx.fill().unwrap();

    let normal_font_face =
        FontFace::toy_create("Sans", FontSlant::Normal, FontWeight::Normal).unwrap();
    let bold_font_face = FontFace::toy_create("Sans", FontSlant::Normal, FontWeight::Bold).unwrap();

    // sector name
    ctx.set_font_size(3.0 * SCALE);
    ctx.set_font_face(&bold_font_face);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
    let text = &sector.name;
    let extents = ctx.text_extents(text).unwrap();
    ctx.move_to(width / SCALE / 4.0 - extents.width / 2.0, 3.0 * SCALE);
    ctx.show_text(text).unwrap();

    // neighboring sector names, if known

    // coreward (up)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0, sector.location.1 - 1))
    {
        draw_neighboring_sector_name(
            &ctx,
            &normal_font_face,
            &neighbor_sector.name,
            width / SCALE / 2.0,
            6.0 * SCALE,
        );
    }

    // spinward (left)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0 - 1, sector.location.1))
    {
        draw_neighboring_sector_name(
            &ctx,
            &normal_font_face,
            &neighbor_sector.name,
            5.0 * SCALE,
            height / SCALE / 2.0,
        );
    }

    // trailing (right)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0 + 1, sector.location.1))
    {
        draw_neighboring_sector_name(
            &ctx,
            &normal_font_face,
            &neighbor_sector.name,
            width / SCALE - 2.0 * SCALE,
            height / SCALE / 2.0,
        );
    }

    // rimward (down)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0, sector.location.1 + 1))
    {
        draw_neighboring_sector_name(
            &ctx,
            &normal_font_face,
            &neighbor_sector.name,
            width / SCALE / 2.0,
            height / SCALE - 6.0 * SCALE,
        );
    }

    // subsector borders
    ctx.set_line_width(0.03 * SCALE);
    ctx.set_source_rgba(0.5, 0.5, 0.5, 1.0); // gray

    // vertical lines
    for x in vec![1.0, 9.0, 17.0, 25.0, 33.0] {
        let cx = (25.0 / 6.0 + x) * 3.0 * SCALE; // halfway between leftmost 2 points
        let y = 1.0;
        let cy1 = (3.0 + y * 2.0) * SQRT3 * SCALE;
        let y = 41.0;
        let cy2 = (3.0 + y * 2.0) * SQRT3 * SCALE;
        ctx.move_to(cx, cy1);
        ctx.line_to(cx, cy2);
        ctx.stroke().unwrap();
    }
    // horizontal lines
    for y in vec![1.0, 11.0, 21.0, 31.0, 41.0] {
        let x = 1.0;
        let cy = (3.0 + y * 2.0) * SQRT3 * SCALE;
        let cx1 = (25.0 / 6.0 + x) * 3.0 * SCALE;
        let x = 33.0;
        let cx2 = (25.0 / 6.0 + x) * 3.0 * SCALE;
        ctx.move_to(cx1, cy);
        ctx.line_to(cx2, cy);
        ctx.stroke().unwrap();
    }

    // subsector names
    for row in 0..4 {
        for col in 0..4 {
            let letter = (char::from_u32(4 * row + col + u32::from('A'))).unwrap();
            if let Some(subsector_name) = sector.subsector_letter_to_name.get(&letter) {
                ctx.set_font_size(3.0 * SCALE);
                ctx.set_font_face(&normal_font_face);
                ctx.set_source_rgba(0.5, 0.5, 0.5, 1.0); // gray
                let text = subsector_name;
                let extents = ctx.text_extents(text).unwrap();
                let x = 8.0 * col as f64 + 5.0;
                let yy = 10.0 * row as f64 + 5.5;
                let cx = (4.0 + x) * 3.0 * SCALE; // leftmost point
                let cy = (5.0 + yy * 2.0) * SQRT3 * SCALE; // topmost point
                ctx.move_to(cx - extents.width / 2.0, cy - extents.height / 2.0);
                ctx.show_text(text).unwrap();
            }
        }
    }

    // hexsides
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let (_hex, _cx, _cy, vertexes, _center, _coords) = init_vars(&sector, x, y);
            ctx.set_line_width(0.03 * SCALE);
            ctx.move_to(vertexes[0].0, vertexes[0].1);
            ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
            for ii in vec![1, 2, 3, 4, 5, 0] {
                ctx.line_to(vertexes[ii].0, vertexes[ii].1);
            }
            ctx.stroke().unwrap();
        }
    }

    // Xboat routes
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let (_hex, cx, cy, _vertexes, center, coords_opt) = init_vars(&sector, x, y);
            if let Some(coords) = coords_opt {
                if let Some(world) = coords_to_world.get(&coords) {
                    draw_route(
                        &ctx,
                        *coords,
                        &world.xboat_routes,
                        0.3 * SCALE,
                        (0.5, 0.0, 0.5, 1.0),
                        cx,
                        cy,
                        center,
                    );
                }
            }
        }
    }

    // trade routes
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let (_hex, cx, cy, _vertexes, center, coords_opt) = init_vars(&sector, x, y);
            if let Some(coords) = coords_opt {
                if let Some(world) = coords_to_world.get(&coords) {
                    draw_route(
                        &ctx,
                        *coords,
                        &world.major_routes,
                        0.09 * SCALE,
                        (0.0, 0.0, 1.0, 1.0),
                        cx,
                        cy,
                        center,
                    );
                    draw_route(
                        &ctx,
                        *coords,
                        &world.main_routes,
                        0.08 * SCALE,
                        (0.0, 0.8, 1.8, 1.0),
                        cx,
                        cy,
                        center,
                    );
                    draw_route(
                        &ctx,
                        *coords,
                        &world.intermediate_routes,
                        0.07 * SCALE,
                        (0.0, 1.0, 0.0, 1.0),
                        cx,
                        cy,
                        center,
                    );
                    draw_route(
                        &ctx,
                        *coords,
                        &world.feeder_routes,
                        0.06 * SCALE,
                        (1.0, 1.0, 0.0, 1.0),
                        cx,
                        cy,
                        center,
                    );
                    draw_route(
                        &ctx,
                        *coords,
                        &world.minor_routes,
                        0.05 * SCALE,
                        (1.0, 0.0, 0.0, 1.0),
                        cx,
                        cy,
                        center,
                    );
                }
            }
        }
    }

    let mut rng = thread_rng();

    // World, gas giants, text
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let (hex, cx, cy, _vertexes, center, coords_opt) = init_vars(&sector, x, y);
            if let Some(coords) = coords_opt {
                if let Some(world) = coords_to_world.get(&coords) {
                    // UWP
                    ctx.set_font_size(0.35 * SCALE);
                    ctx.set_font_face(&normal_font_face);
                    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
                    let text = &world.uwp;
                    let extents = ctx.text_extents(&text).unwrap();
                    ctx.move_to(
                        cx + 2.0 * SCALE - extents.width / 2.0,
                        cy + SQRT3 * SCALE * 1.5,
                    );
                    ctx.show_text(&text).unwrap();

                    // World name
                    // All-caps for high population
                    let name: String;
                    if world.population().is_alphabetic() || world.population() == '9' {
                        name = world.name.to_owned().to_uppercase();
                    } else {
                        name = world.name.to_owned();
                    }
                    ctx.set_font_size(0.4 * SCALE);
                    ctx.set_font_face(&bold_font_face);
                    let extents = ctx.text_extents(&name).unwrap();
                    // Red if a sector or subsector capital
                    if world.trade_classifications.contains("Cp")
                        || world.trade_classifications.contains("Cs")
                    {
                        ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0); // red
                    } else {
                        ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
                    }
                    ctx.move_to(
                        cx + 2.0 * SCALE - extents.width / 2.0,
                        cy + SQRT3 * SCALE * 1.75,
                    );
                    ctx.show_text(&name).unwrap();

                    // DWTN, endpoint trace BTN, transient trade BTN, port size
                    ctx.set_font_size(0.35 * SCALE);
                    ctx.set_font_face(&normal_font_face);
                    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0); // white
                    let dwtn = (world.wtn() * 2.0) as u64;
                    let endpoint_dbtn =
                        bisect_left(&DBTN_TO_CREDITS, &world.endpoint_trade_credits);
                    let endpoint_btn = endpoint_dbtn / 2;
                    let transient_dbtn =
                        bisect_left(&DBTN_TO_CREDITS, &world.transient_trade_credits);
                    let transient_btn = transient_dbtn / 2;
                    let text = format!(
                        "{:X}{:X}{:X}{:X}",
                        dwtn,
                        endpoint_btn,
                        transient_btn,
                        world.port_size()
                    );
                    let extents = ctx.text_extents(&text).unwrap();
                    ctx.move_to(
                        cx + 2.0 * SCALE - extents.width / 2.0,
                        cy + SQRT3 * SCALE * 1.95,
                    );
                    ctx.show_text(&text).unwrap();

                    // World circle
                    if world.size() == '0' {
                        // Asteroid belt
                        let rgba = (1.0, 1.0, 1.0, 1.0); // white
                        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
                        let num_asteroids: u64 = rng.gen_range(5..=20);
                        for _ in 0..num_asteroids {
                            let x_pos = center.0 - 0.25 * SCALE + random::<f64>() * 0.5 * SCALE;
                            let y_pos = center.1 - 0.25 * SCALE + random::<f64>() * 0.5 * SCALE;
                            ctx.new_sub_path();
                            ctx.arc(x_pos, y_pos, random::<f64>() * 0.04 * SCALE, 0.0, TAU);
                            ctx.stroke_preserve().unwrap();
                            ctx.fill().unwrap();
                        }
                    } else {
                        let mut rgba = (1.0, 1.0, 1.0, 1.0); // white
                        let mut fill_rgba = rgba;
                        if world.trade_classifications.contains("Ri")
                            && world.trade_classifications.contains("Ag")
                        {
                            rgba = (1.0, 1.0, 0.0, 1.0); // yellow
                            fill_rgba = rgba;
                        } else if world.trade_classifications.contains("Ri") {
                            rgba = (0.5, 0.0, 0.5, 1.0); // purple
                            fill_rgba = rgba;
                        } else if world.trade_classifications.contains("Ag") {
                            rgba = (0.5, 0.5, 0.5, 1.0); // green
                            fill_rgba = rgba;
                        } else if world.atmosphere() == 'B' || world.atmosphere() == 'C' {
                            rgba = (1.0, 0.65, 0.0, 1.0); // orange
                            fill_rgba = rgba;
                        } else if world.atmosphere() == '0' {
                            fill_rgba = (0.0, 0.0, 0.0, 1.0); // black
                        } else if world.hydrosphere() != '0' {
                            rgba = (0.0, 0.0, 1.0, 1.0); // blue
                            fill_rgba = rgba;
                        }
                        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
                        ctx.new_sub_path();
                        ctx.arc(center.0, center.1, 0.3 * SCALE, 0.0, TAU);
                        ctx.stroke_preserve().unwrap();
                        if fill_rgba != rgba {
                            ctx.set_source_rgba(fill_rgba.0, fill_rgba.1, fill_rgba.2, fill_rgba.3);
                        }
                        ctx.fill().unwrap();
                    }

                    // Gas giant
                    if world.gas_giants() != '0' {
                        let rgba = (1.0, 1.0, 1.0, 1.0); // white
                        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
                        ctx.new_sub_path();
                        ctx.arc(
                            center.0 + 0.8 * SCALE,
                            center.1 - 0.8 * SCALE,
                            0.1 * SCALE,
                            0.0,
                            TAU,
                        );
                        ctx.set_line_width(0.03 * SCALE);
                        ctx.stroke_preserve().unwrap();
                        ctx.fill().unwrap();
                    }

                    // Red and amber zones
                    if world.zone == 'R' || world.zone == 'A' {
                        let mut rgba = (1.0, 0.0, 0.0, 1.0); // red
                        if world.zone == 'A' {
                            rgba = (1.0, 1.0, 0.0, 1.0); // yellow
                        }
                        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
                        ctx.new_sub_path();
                        ctx.arc(center.0, center.1, 1.5 * SCALE, 0.7 * PI, 2.3 * PI);
                        ctx.set_line_width(0.03 * SCALE);
                        ctx.stroke().unwrap();
                    }
                }

                // Hex label
                let text = hex;
                ctx.set_font_size(0.35 * SCALE);
                ctx.set_font_face(&normal_font_face);
                let extents = ctx.text_extents(&text).unwrap();
                let rgba = (1.0, 1.0, 1.0, 1.0); // white
                ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
                ctx.move_to(
                    cx + 2.0 * SCALE - extents.width / 2.0,
                    cy + SQRT3 * SCALE * 0.3,
                );
                ctx.show_text(&text).unwrap();
            }
        }
    }

    surface.finish();
}

fn generate_pdfs(
    output_dir: &PathBuf,
    location_to_sector: &HashMap<(i64, i64), Sector>,
    coords_to_world: &HashMap<Coords, World>,
) {
    debug!("(parallel) generate_pdfs");
    location_to_sector
        .par_iter()
        .map(|(_, sector)| generate_pdf(sector, output_dir, location_to_sector, coords_to_world))
        .collect::<Vec<()>>();
}

/// Absolute coordinates
/// x is an integer
/// y2 is an integer, equal to 2 * y
/// This is needed because y is sometimes a float and floats can't be hash keys
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Coords {
    x: i64,
    y2: i64,
}

impl Coords {
    fn new(xf: f64, yf: f64) -> Coords {
        let x = xf as i64;
        let y2 = (yf * 2.0) as i64;
        Coords { x, y2 }
    }

    fn straight_line_distance(&self, other: &Coords) -> u32 {
        let (x1, y1) = <(f64, f64)>::from(*self);
        let (x2, y2) = <(f64, f64)>::from(*other);
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        return (f64::floor(xdelta + ydelta)) as u32;
    }
}

impl From<Coords> for (f64, f64) {
    fn from(coords: Coords) -> (f64, f64) {
        let fx = coords.x as f64;
        let fy = coords.y2 as f64 / 2.0;
        (fx, fy)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct World {
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
    neighbors1: HashSet<Coords>,
    neighbors2: HashSet<Coords>,
    neighbors3: HashSet<Coords>,
    index: Option<usize>,
}

impl World {
    fn new(
        line: String,
        fields: &Vec<(usize, usize, String)>,
        sector_location: (i64, i64),
    ) -> World {
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
            let set = HashSet::new();
            dbtn_to_coords.push(set);
        }
        let major_routes = HashSet::new();
        let main_routes = HashSet::new();
        let intermediate_routes = HashSet::new();
        let feeder_routes = HashSet::new();
        let minor_routes = HashSet::new();
        let neighbors1 = HashSet::new();
        let neighbors2 = HashSet::new();
        let neighbors3 = HashSet::new();
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
                    if trimmed.len() > 0 {
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
                    if trimmed.len() > 0 {
                        for ch in trimmed.chars() {
                            bases.insert(ch.to_string());
                        }
                    }
                }
                "Z" => {
                    let trimmed = value.trim_matches(|c| c == ' ' || c == '-').to_string();
                    if trimmed.len() > 0 {
                        zone = trimmed.chars().nth(0).unwrap();
                    }
                }
                "PBG" => pbg = value.trim().to_string(),
                "W" => {
                    let trimmed = value
                        .trim_matches(|c| c == '{' || c == '}' || c == ' ')
                        .to_string();
                    if trimmed.len() > 0 {
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
                            stars.push(star.to_owned() + " " + &parts[ii + 1]);
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

        let world = World {
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
            neighbors1,
            neighbors2,
            neighbors3,
            index,
        };
        world
    }

    /// Find and cache all neighbors within 3 hexes.
    ///
    /// This must be run after all Sectors and Worlds are mostly initialized.
    fn populate_neighbors(&mut self, coords_to_world: &HashMap<Coords, World>) {
        if !self.can_refuel() {
            return;
        }
        let (x, y) = <(f64, f64)>::from(self.get_coords());
        let mut xx = x - 3.0;
        while xx <= x + 3.0 {
            let mut yy = y - 3.0;
            while yy <= y + 3.0 {
                let world_opt = coords_to_world.get(&Coords::new(xx, yy));
                if let Some(world) = world_opt {
                    if world != self && world.can_refuel() {
                        let distance = self.straight_line_distance(world);
                        match distance {
                            1 => self.neighbors1.insert(world.get_coords()),
                            2 => self.neighbors2.insert(world.get_coords()),
                            3 => self.neighbors3.insert(world.get_coords()),
                            _ => false,
                        };
                    }
                }
                yy += 0.5;
            }
            xx += 1.0;
        }
    }

    fn starport(&self) -> char {
        return self.uwp.chars().nth(0).unwrap() as char;
    }

    fn g_starport(&self) -> String {
        let mut starport = self.starport();
        if starport == '?' {
            starport = 'X';
        }
        let opt = STARPORT_TRAVELLER_TO_GURPS.get(&starport);
        return opt.unwrap().to_string();
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
        if tech_level_char == '?' {
            tech_level_char = '0';
        }
        let tech_level_int = tech_level_char.to_digit(MAX_TECH_LEVEL + 1).unwrap();
        return *TECH_LEVEL_TRAVELLER_TO_GURPS.get(&tech_level_int).unwrap();
    }

    fn gas_giants(&self) -> char {
        return self.pbg.chars().nth(2).unwrap();
    }

    fn can_refuel(&self) -> bool {
        return self.gas_giants() != '0'
            || (self.zone != 'R'
                && ((self.starport() != 'E' && self.starport() != 'X')
                    || self.hydrosphere() != '0'));
    }

    fn uwtn(&self) -> f64 {
        let gt3 = self.g_tech_level() / 3;
        let tl_mod = gt3 as f64 / 2.0 - 0.5;
        let pop_char = self.population();
        let mut pop_mod = 0.0;
        if pop_char.is_alphanumeric() {
            // ignore '?'
            let pop_int = pop_char.to_digit(MAX_POPULATION + 1).unwrap();
            pop_mod = pop_int as f64 / 2.0;
        }
        return tl_mod + pop_mod as f64;
    }

    fn wtn_port_modifier(&self) -> f64 {
        let iuwtn = u64::min(7, u64::max(0, self.uwtn() as u64));
        return *WTN_PORT_MODIFIER_TABLE
            .get(&(iuwtn, self.g_starport()))
            .unwrap();
    }

    fn wtn(&self) -> f64 {
        return self.uwtn() + self.wtn_port_modifier();
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
        scratch.push(hex.chars().nth(0).unwrap());
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
        return Coords { x, y2 };
    }

    fn straight_line_distance(&self, other: &World) -> i32 {
        let (x1, y1) = <(f64, f64)>::from(self.get_coords());
        let (x2, y2) = <(f64, f64)>::from(other.get_coords());
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        return (f64::floor(xdelta + ydelta)) as i32;
    }

    fn navigable_distance(&self, other: &World, dist: &Array2<i32>) -> i32 {
        let ii = self.index.unwrap();
        let jj = other.index.unwrap();
        return dist[[ii, jj]];
    }

    fn navigable_path(
        &self,
        other: &World,
        sorted_coords: &Vec<Coords>,
        coords_to_index: &HashMap<Coords, usize>,
        dist: &Array2<i32>,
        pred: &Array2<i32>,
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
            coords2 = sorted_coords[index as usize].clone();
            if coords2 == other.get_coords() {
                path.push(coords2);
                break;
            } else {
                path.push(coords2);
            }
        }
        return Some(path);
    }

    fn distance_modifier(&self, other: &World, dist2: &Array2<i32>) -> f64 {
        let distance = self.navigable_distance(other, dist2);
        distance_modifier_table(distance)
    }

    fn btn(&self, other: &World, dist2: &Array2<i32>) -> f64 {
        let wtn1 = self.wtn();
        let wtn2 = other.wtn();
        let min_wtn = f64::min(wtn1, wtn2);
        let base_btn = wtn1 + wtn2 + self.wtcm(other);
        let btn = base_btn - self.distance_modifier(other, dist2);
        f64::max(MIN_BTN, f64::min(btn, min_wtn + MAX_BTN_WTN_DELTA))
    }

    fn passenger_btn(&self, other: &World, dist2: &Array2<i32>) -> f64 {
        let wtn1 = self.wtn();
        let wtn2 = other.wtn();
        let min_wtn = f64::min(wtn1, wtn2);
        let base_btn = wtn1 + wtn2 + self.wtcm(other);
        let mut pbtn = base_btn - self.distance_modifier(other, dist2);
        for world in vec![self, other] {
            if world.trade_classifications.contains("Ri") {
                pbtn += RI_PBTN_BONUS;
            }
            if world.trade_classifications.contains("Cp") {
                pbtn += CP_PBTN_BONUS;
            }
            if world.trade_classifications.contains("Cs") {
                pbtn += CS_PBTN_BONUS;
            }
        }
        f64::max(MIN_BTN, f64::min(pbtn, min_wtn + MAX_BTN_WTN_DELTA))
    }

    fn find_route_paths(
        &self,
        sorted_coords: &Vec<Coords>,
        coords_to_world: &HashMap<Coords, World>,
        coords_to_index: &HashMap<Coords, usize>,
        dist2: &Array2<i32>,
        pred2: &Array2<i32>,
        dist3: &Array2<i32>,
        pred3: &Array2<i32>,
    ) -> (HashMap<(Coords, Coords), u64>, HashMap<Coords, u64>) {
        let mut route_paths: HashMap<(Coords, Coords), u64> = HashMap::new();
        let mut coords_to_transient_credits: HashMap<Coords, u64> = HashMap::new();
        for (dbtn, coords_set) in self.dbtn_to_coords.iter().enumerate() {
            let credits = DBTN_TO_CREDITS[dbtn];
            for coords2 in coords_set {
                let world2 = coords_to_world.get(&coords2).unwrap();
                let mut path: Vec<Coords> = Vec::new();
                let possible_path2 =
                    self.navigable_path(world2, sorted_coords, coords_to_index, dist2, pred2);
                let mut possible_path3 = None;
                if dbtn >= MIN_DBTN_FOR_JUMP_3 {
                    possible_path3 =
                        self.navigable_path(world2, sorted_coords, coords_to_index, dist3, pred3);
                }
                if let Some(path2) = possible_path2 {
                    path = path2;
                    if let Some(path3) = possible_path3 {
                        if path3.len() < path.len() {
                            path = path3;
                        }
                    }
                } else if let Some(path3) = possible_path3 {
                    path = path3;
                }
                if path.len() >= 2 {
                    for ii in 0..path.len() - 1 {
                        let first = path.get(ii).unwrap();
                        let second = path.get(ii + 1).unwrap();
                        let coord_tup: (Coords, Coords);
                        if first <= second {
                            coord_tup = (*first, *second);
                        } else {
                            coord_tup = (*second, *first);
                        }
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
        self.allegiance == "CsIm"
            || self.allegiance.chars().nth(0).unwrap() == 'I'
                && self.allegiance.chars().nth(1).unwrap() == 'm'
    }

    // This only works after trade routes are built.
    fn port_size(&self) -> u64 {
        let mut port_size = self.wtn();
        if !self.imperial_affiliated() {
            port_size -= NON_IMPERIAL_PORT_SIZE_PENALTY;
        }
        if self.neighbors1.len() > 0 {
            port_size += NEIGHBOR_1_PORT_SIZE_BONUS;
        } else if self.neighbors2.len() > 0 {
            port_size += NEIGHBOR_2_PORT_SIZE_BONUS;
        }
        port_size = f64::ceil(port_size);
        if self.xboat_routes.len() > 0 || self.major_routes.len() > 0 {
            if port_size < XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE {
                port_size = XBOAT_MAJOR_ROUTE_MIN_PORT_SIZE;
            }
        } else if self.main_routes.len() > 0
            || self.intermediate_routes.len() > 0
            || self.feeder_routes.len() > 0
        {
            if port_size < FEEDER_ROUTE_MIN_PORT_SIZE {
                port_size = FEEDER_ROUTE_MIN_PORT_SIZE;
            }
        } else if self.minor_routes.len() > 0 {
            if port_size < MINOR_ROUTE_MIN_PORT_SIZE {
                port_size = MINOR_ROUTE_MIN_PORT_SIZE;
            }
        }
        port_size as u64
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
struct Sector {
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
        data_dir: &PathBuf,
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

        sector.parse_xml_metadata(&data_dir, &sector_name).unwrap();
        sector
            .parse_column_data(&data_dir, &sector_name, coords_to_world)
            .unwrap();
        sector
    }

    fn parse_xml_metadata(&mut self, data_dir: &PathBuf, sector_name: &str) -> Result<()> {
        let mut xml_path = data_dir.clone();
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
            if name_element.text().len() > 0 {
                self.names.push(name_element.text().to_string());
            }
        }

        let subsectors_opt = root.find("Subsectors");
        if let Some(subsectors_element) = subsectors_opt {
            let subsector_elements = subsectors_element.find_all("Subsector");
            for subsector_element in subsector_elements {
                let index_opt = subsector_element.get_attr("Index");
                if let Some(index) = index_opt {
                    let letter = index.chars().nth(0).unwrap();
                    let subsector_name = subsector_element.text().to_string();
                    if subsector_name.len() > 0 {
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
                    if allegiance_name.len() > 0 {
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
        data_dir: &PathBuf,
        sector_name: &str,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Result<()> {
        let mut data_path = data_dir.clone();
        data_path.push(sector_name.to_owned() + ".sec");
        let blob = read_to_string(data_path)?;
        let mut header = "";
        // We initialize fields here to make rustc happy, then overwrite it.
        let mut fields: Vec<(usize, usize, String)> = Vec::new();
        for line in blob.lines() {
            if line.len() == 0 || line.starts_with("#") {
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
        data_dir: &PathBuf,
        location_to_sector: &HashMap<(i64, i64), Sector>,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Result<()> {
        let mut xml_path = data_dir.clone();
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
    let sector_names: Vec<String> = sector_names_set.into_iter().collect();

    let verbose = args.verbose;
    let quiet = args.quiet;
    if quiet && verbose > 0 {
        eprintln!("Please do not set both --quiet and --verbose.  Exiting");
        exit(1);
    }

    stderrlog::new()
        .module(module_path!())
        .quiet(quiet)
        .verbosity(verbose)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .init()
        .unwrap();

    if sector_names.len() == 0 {
        error!("No sectors.  Exiting.");
        exit(2);
    }
    debug!("{} sectors", sector_names.len());

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
            world.populate_neighbors(&coords_to_world2);
        }
    }
    let mut sorted_coords: Vec<Coords>;
    sorted_coords = coords_to_world.keys().cloned().collect();
    sorted_coords.sort();
    let mut coords_to_index: HashMap<Coords, usize> = HashMap::new();
    for (ii, coords) in sorted_coords.iter_mut().enumerate() {
        coords_to_index.insert(coords.clone(), ii);
        let world = coords_to_world.get_mut(coords).unwrap();
        world.index = Some(ii);
    }
    let (dist2, pred2) = populate_navigable_distances(&sorted_coords, &coords_to_world, 2);
    let (dist3, pred3) = populate_navigable_distances(&sorted_coords, &coords_to_world, 3);

    populate_trade_routes(
        &mut coords_to_world,
        &coords_to_index,
        &sorted_coords,
        &dist2,
        &pred2,
        &dist3,
        &pred3,
    );

    generate_pdfs(&output_dir, &location_to_sector, &coords_to_world);

    temp_dir.close()?;

    debug!("Exit");

    Ok(())
}
