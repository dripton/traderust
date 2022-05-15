use anyhow::Result;
use bisection::bisect_left;
use clap::Parser;
use clap_verbosity_flag;
use elementtree::Element;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
#[macro_use]
extern crate lazy_static;
extern crate ndarray;
use ndarray::prelude::*;
extern crate reqwest;
use substring::Substring;
use tempfile::tempdir;
use url::Url;

mod apsp;
use apsp::{dijkstra, INFINITY};
#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, multiple_occurrences = true, required = true)]
    sector: Vec<String>,

    /// Path to input matrix in numpy ndy format
    #[clap(short, long)]
    data_directory: Option<PathBuf>,

    /// Path to output distance and predecessor matrixes in numpy ndz format
    #[clap(short, long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

const MAX_TECH_LEVEL: u32 = 17;
const MAX_POPULATION: u32 = 15;

// Rules don't say BTN can't be negative but it seems reasonable to me.
const MIN_BTN: f64 = 0.0;
const MAX_BTN_WTN_DELTA: f64 = 5.0;

const _RI_PBTN_BONUS: f64 = 0.5;
const _CP_PBTN_BONUS: f64 = 0.5;
const _CS_PBTN_BONUS: f64 = 0.5;

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

lazy_static! {
    static ref SQRT3: f64 = f64::powf(3.0, 0.5);

    static ref STARPORT_TRAVELLER_TO_GURPS: HashMap<String, String> = {
        let mut sttg: HashMap<String, String> = HashMap::new();
        sttg.insert("A".to_string(), "V".to_string());
        sttg.insert("B".to_string(), "IV".to_string());
        sttg.insert("C".to_string(), "III".to_string());
        sttg.insert("D".to_string(), "II".to_string());
        sttg.insert("E".to_string(), "I".to_string());
        sttg.insert("X".to_string(), "0".to_string());
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
}

fn download_sector_data(data_dir: &PathBuf, sector_names: &Vec<String>) -> Result<()> {
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
            let body = reqwest::blocking::get(data_url)?.text()?;
            write(data_path, body)?;
        }
        if !metadata_path.exists() {
            let metadata_url = base_url.join(&(sector_name.to_owned() + "/metadata"))?;
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
) -> (Array2<i64>, Array2<i64>) {
    let num_worlds = sorted_coords.len();
    let mut np = Array2::<i64>::zeros((num_worlds, num_worlds));
    for (ii, coords) in sorted_coords.iter().enumerate() {
        let world_opt = coords_to_world.get(coords);
        if let Some(world) = world_opt {
            if max_jump >= 3 {
                for coords in &world.neighbors3 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 3;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            if max_jump >= 2 {
                for coords in &world.neighbors2 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 2;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            if max_jump >= 1 {
                for coords in &world.neighbors1 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 1;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            for coords in &world.xboat_routes {
                if let Some(neighbor) = coords_to_world.get(&coords) {
                    if let Some(jj) = neighbor.index {
                        np[[ii, jj]] = world.straight_line_distance(neighbor) as i64;
                    } else {
                        panic!("neighbor with no index");
                    }
                } else {
                    panic!("missing neighbor at index");
                }
            }
        } else {
            panic!("Failed to get world");
        }
    }

    let pred = dijkstra(&mut np);
    return (np, pred);
}

fn distance_modifier_table(distance: i64) -> f64 {
    let table: Vec<i64> = vec![1, 2, 5, 9, 19, 29, 59, 99, 199, 299, 599, 999, INFINITY];
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
    dist2: &Array2<i64>,
    pred2: &Array2<i64>,
    dist3: &Array2<i64>,
    pred3: &Array2<i64>,
) {
    let mut dwtn_coords: Vec<(u64, Coords)> = Vec::new();
    for (coords, world) in coords_to_world.iter() {
        // wtn can have 0.5 so double it to make a sortable integer
        let dwtn = (world.wtn() * 2.0) as u64;
        dwtn_coords.push((dwtn, *coords));
    }
    dwtn_coords.sort();
    dwtn_coords.reverse();

    // Add initial endpoint-only routes to both endpoints
    for (ii, (dwtn1, coords1)) in dwtn_coords.iter().enumerate() {
        let wtn1 = *dwtn1 as f64 / 2.0;
        for jj in ii + 1..dwtn_coords.len() {
            let (dwtn2, coords2) = dwtn_coords[jj];
            let wtn2 = dwtn2 as f64 / 2.0;
            if wtn2 < MINOR_ROUTE_THRESHOLD - MAX_BTN_WTN_DELTA
                || wtn1 + wtn2 < MINOR_ROUTE_THRESHOLD - MAX_WTCM_BONUS
            {
                // BTN can't be more than the lower WTN + 5, or the sum of
                // the WTNs plus 1.  So if the lower WTN is less than 3 or
                // the sum of the WTNs is less than 7, we know that world2
                // and later worlds won't form any trade routes with
                // world1.
                break;
            }
            let sld = coords1.straight_line_distance(&coords2) as i64;
            let max_btn1 = wtn1 + wtn2 - distance_modifier_table(sld);
            if max_btn1 < MINOR_ROUTE_THRESHOLD - MAX_WTCM_BONUS {
                // BTN can't be more than the sum of the WTNs plus 1, so if
                // even the straight line distance modifier puts us below 7,
                // we can't form any trade routes with world2.
                continue;
            }
            let world1 = coords_to_world.get(&coords1).unwrap();
            let world2 = coords_to_world.get(&coords2).unwrap();
            if max_btn1 < MINOR_ROUTE_THRESHOLD + MAX_WTCM_PENALTY {
                // Computing the wtcm is cheaper than finding the full BTN
                let wtcm = world1.wtcm(&world2);
                let max_btn2 = max_btn1 + wtcm;
                if max_btn2 < MINOR_ROUTE_THRESHOLD {
                    continue;
                }
            }
            // At this point we have exhausted ways to skip world2 without
            // computing the BTN.
            let btn = world1.btn(&world2, dist2);
            if btn >= MAJOR_ROUTE_THRESHOLD {
                coords_to_world
                    .get_mut(&coords1)
                    .unwrap()
                    .major_routes
                    .insert(coords2);
                coords_to_world
                    .get_mut(&coords2)
                    .unwrap()
                    .major_routes
                    .insert(*coords1);
            } else if btn >= MAIN_ROUTE_THRESHOLD {
                coords_to_world
                    .get_mut(&coords1)
                    .unwrap()
                    .main_routes
                    .insert(coords2);
                coords_to_world
                    .get_mut(&coords2)
                    .unwrap()
                    .main_routes
                    .insert(*coords1);
            } else if btn >= INTERMEDIATE_ROUTE_THRESHOLD {
                coords_to_world
                    .get_mut(&coords1)
                    .unwrap()
                    .intermediate_routes
                    .insert(coords2);
                coords_to_world
                    .get_mut(&coords2)
                    .unwrap()
                    .intermediate_routes
                    .insert(*coords1);
            } else if btn >= FEEDER_ROUTE_THRESHOLD {
                coords_to_world
                    .get_mut(&coords1)
                    .unwrap()
                    .feeder_routes
                    .insert(coords2);
                coords_to_world
                    .get_mut(&coords2)
                    .unwrap()
                    .feeder_routes
                    .insert(*coords1);
            } else if btn >= MINOR_ROUTE_THRESHOLD {
                coords_to_world
                    .get_mut(&coords1)
                    .unwrap()
                    .minor_routes
                    .insert(coords2);
                coords_to_world
                    .get_mut(&coords2)
                    .unwrap()
                    .minor_routes
                    .insert(*coords1);
            }
        }
    }

    for (_, coords) in dwtn_coords {
        let world = coords_to_world.get(&coords).unwrap();
        let major_route_paths = world.find_route_paths(
            &world.major_routes,
            3,
            &sorted_coords,
            &coords_to_world,
            &coords_to_index,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );
        let main_route_paths = world.find_route_paths(
            &world.main_routes,
            3,
            &sorted_coords,
            &coords_to_world,
            &coords_to_index,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );
        let intermediate_route_paths = world.find_route_paths(
            &world.intermediate_routes,
            3,
            &sorted_coords,
            &coords_to_world,
            &coords_to_index,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );
        let feeder_route_paths = world.find_route_paths(
            &world.feeder_routes,
            3,
            &sorted_coords,
            &coords_to_world,
            &coords_to_index,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );
        let minor_route_paths = world.find_route_paths(
            &world.minor_routes,
            2,
            &sorted_coords,
            &coords_to_world,
            &coords_to_index,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );
    }

    // TODO Promote routes
    // TODO Keep only the largest route for each pair of coords
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

    fn straight_line_distance(&self, other: &Coords) -> u64 {
        let (x1, y1) = <(f64, f64)>::from(*self);
        let (x2, y2) = <(f64, f64)>::from(*other);
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        return (f64::floor(xdelta + ydelta)) as u64;
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
    zone: String,
    pbg: String,
    worlds: u64,
    allegiance: String,
    stars: Vec<String>,
    xboat_routes: HashSet<Coords>,
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
        let mut zone = "G".to_string();
        let mut pbg = "".to_string();
        let mut worlds = 0;
        let mut allegiance = "".to_string();
        let mut stars = Vec::new();
        let xboat_routes = HashSet::new();
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
                        zone = trimmed;
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
                        if star == "BD" || star == "D" {
                            stars.push(star.to_owned());
                            ii += 1;
                        } else {
                            stars.push(star.to_owned() + " " + &parts[ii + 1]);
                            ii += 2;
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
            xboat_routes,
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

    fn starport(&self) -> String {
        return self.uwp.substring(0, 1).to_string();
    }

    fn g_starport(&self) -> String {
        let starport = self.starport();
        let opt = STARPORT_TRAVELLER_TO_GURPS.get(&starport);
        return opt.unwrap().to_string();
    }

    fn size(&self) -> String {
        return self.uwp.substring(1, 2).to_string();
    }

    fn atmosphere(&self) -> String {
        return self.uwp.substring(2, 3).to_string();
    }

    fn hydrosphere(&self) -> String {
        return self.uwp.substring(3, 4).to_string();
    }

    fn population(&self) -> String {
        return self.uwp.substring(4, 5).to_string();
    }

    fn government(&self) -> String {
        return self.uwp.substring(5, 6).to_string();
    }

    fn law_level(&self) -> String {
        return self.uwp.substring(6, 7).to_string();
    }

    fn tech_level(&self) -> String {
        return self.uwp.substring(8, 9).to_string();
    }

    fn g_tech_level(&self) -> u64 {
        let tech_level_string = self.tech_level();
        let mut tech_level_int = 0;
        for ch in tech_level_string.chars() {
            tech_level_int = ch.to_digit(MAX_TECH_LEVEL + 1).unwrap();
            break;
        }
        return *TECH_LEVEL_TRAVELLER_TO_GURPS.get(&tech_level_int).unwrap();
    }

    fn gas_giants(&self) -> String {
        return self.pbg.substring(2, 3).to_string();
    }

    fn can_refuel(&self) -> bool {
        return self.gas_giants() != "0"
            || (self.zone != "R"
                && ((self.starport() != "E" && self.starport() != "X")
                    || self.hydrosphere() != "0"));
    }

    fn uwtn(&self) -> f64 {
        let gt3 = self.g_tech_level() / 3;
        let tl_mod = gt3 as f64 / 2.0 - 0.5;
        let mut population_int = 0;
        for ch in self.population().chars() {
            population_int = ch.to_digit(MAX_POPULATION + 1).unwrap();
            break;
        }
        let pop_mod = population_int as f64 / 2.0;
        return tl_mod + pop_mod as f64;
    }

    fn wtn_port_modifier(&self) -> f64 {
        let iuwtn = u64::max(0, self.uwtn() as u64);
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
        let x: i64 = hex.substring(0, 2).parse::<i64>().unwrap() + 32 * location.0;
        let y: i64 = hex.substring(2, 4).parse::<i64>().unwrap() + 40 * location.1;
        let mut y2 = 2 * y;
        if x & 1 == 0 {
            y2 += 1;
        }
        return Coords { x, y2 };
    }

    fn straight_line_distance(&self, other: &World) -> u64 {
        let (x1, y1) = <(f64, f64)>::from(self.get_coords());
        let (x2, y2) = <(f64, f64)>::from(other.get_coords());
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        return (f64::floor(xdelta + ydelta)) as u64;
    }

    fn navigable_distance(&self, other: &World, dist: &Array2<i64>) -> i64 {
        if let Some(ii) = self.index {
            if let Some(jj) = other.index {
                return dist[[ii, jj]];
            } else {
                panic!("navigable_distance without index set");
            }
        } else {
            panic!("navigable_distance without index set");
        }
    }

    fn navigable_path(
        &self,
        other: &World,
        sorted_coords: &Vec<Coords>,
        coords_to_index: &HashMap<Coords, usize>,
        dist: &Array2<i64>,
        pred: &Array2<i64>,
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
            if let Some(ii) = other.index {
                if let Some(jj) = coords_to_index.get(&coords2) {
                    let index = pred[[ii, *jj]];
                    coords2 = sorted_coords[index as usize].clone();
                    if coords2 == other.get_coords() {
                        path.push(coords2);
                        break;
                    } else {
                        path.push(coords2);
                    }
                } else {
                    panic!("navigable_path without index set");
                }
            } else {
                panic!("navigable_path without index set");
            }
        }
        return Some(path);
    }

    fn distance_modifier(&self, other: &World, dist2: &Array2<i64>) -> f64 {
        let distance = self.navigable_distance(other, dist2);
        distance_modifier_table(distance)
    }

    fn btn(&self, other: &World, dist2: &Array2<i64>) -> f64 {
        let wtn1 = self.wtn();
        let wtn2 = other.wtn();
        let min_wtn = f64::min(wtn1, wtn2);
        let base_btn = wtn1 + wtn2 + self.wtcm(other);
        let btn = base_btn - self.distance_modifier(other, dist2);
        f64::max(MIN_BTN, f64::min(btn, min_wtn + MAX_BTN_WTN_DELTA))
    }

    fn find_route_paths(
        &self,
        routes: &HashSet<Coords>,
        max_jump: u64,
        sorted_coords: &Vec<Coords>,
        coords_to_world: &HashMap<Coords, World>,
        coords_to_index: &HashMap<Coords, usize>,
        dist2: &Array2<i64>,
        pred2: &Array2<i64>,
        dist3: &Array2<i64>,
        pred3: &Array2<i64>,
    ) -> HashMap<(Coords, Coords), u64> {
        let mut route_paths: HashMap<(Coords, Coords), u64> = HashMap::new();
        for coords2 in routes {
            let world2 = coords_to_world.get(&coords2).unwrap();
            let mut path: Vec<Coords> = Vec::new();
            let possible_path2 =
                self.navigable_path(world2, sorted_coords, coords_to_index, dist2, pred2);
            let mut possible_path3 = None;
            if max_jump == 3 {
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
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
            }
        }
        route_paths
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
    names: Vec<String>,
    abbreviation: String,
    location: (i64, i64),
    subsector_letter_to_name: HashMap<String, String>,
    allegiance_code_to_name: HashMap<String, String>,
    hex_to_coords: HashMap<String, Coords>,
}

impl Sector {
    fn new(
        data_dir: &PathBuf,
        sector_name: String,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Sector {
        let names = Vec::new();
        let abbreviation = "".to_string();
        let location = (-1, -1);
        let subsector_letter_to_name = HashMap::new();
        let allegiance_code_to_name = HashMap::new();
        let hex_to_coords = HashMap::new();
        let mut sector = Sector {
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
                let letter_opt = subsector_element.get_attr("Index");
                if let Some(letter) = letter_opt {
                    let subsector_name = subsector_element.text().to_string();
                    if subsector_name.len() > 0 {
                        self.subsector_letter_to_name
                            .insert(letter.to_string(), subsector_name);
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
            } else if line.starts_with("----") {
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
        xml_path.push(self.name().to_owned() + ".xml");
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

    fn name(&self) -> &str {
        &self.names[0]
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

fn main() -> Result<()> {
    let args = Args::parse();
    let output_dir = args.output_directory;
    let temp_dir = tempdir()?;
    let mut data_dir: PathBuf = temp_dir.path().to_path_buf();
    if let Some(data_dir_override) = args.data_directory {
        data_dir = data_dir_override;
    };
    let sector_names = args.sector;

    create_dir_all(&output_dir)?;
    create_dir_all(&data_dir)?;

    download_sector_data(&data_dir, &sector_names)?;

    let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
    let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
    for sector_name in sector_names {
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);
        location_to_sector.insert(sector.location, sector);
    }
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
        let world_opt = coords_to_world.get_mut(coords);
        if let Some(world) = world_opt {
            world.index = Some(ii);
        } else {
            panic!("World not found at expected coords");
        }
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

    // TODO Generate PDFs

    temp_dir.close()?;

    Ok(())
}
