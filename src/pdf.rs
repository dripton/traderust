use bisection::bisect_left;

use log::debug;

use rayon::prelude::*;

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use std::path::Path;

extern crate cairo;
use cairo::{Context, FontFace, FontSlant, FontWeight, PdfSurface};

extern crate rand;
use rand::{random, thread_rng, Rng};

use crate::{Coords, Sector, World, DBTN_TO_CREDITS};

const SQRT3: f64 = 1.7320508075688772;

const BLACK: (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 1.0);
const WHITE: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 1.0);
const GRAY: (f64, f64, f64, f64) = (0.5, 0.5, 0.5, 1.0);
const RED: (f64, f64, f64, f64) = (1.0, 0.0, 0.0, 1.0);
const ORANGE: (f64, f64, f64, f64) = (1.0, 0.65, 0.0, 1.0);
const YELLOW: (f64, f64, f64, f64) = (1.0, 1.0, 0.0, 1.0);
const GREEN: (f64, f64, f64, f64) = (0.0, 1.0, 0.0, 1.0);
const CYAN: (f64, f64, f64, f64) = (0.0, 0.8, 0.8, 1.0);
const BLUE: (f64, f64, f64, f64) = (0.0, 0.0, 1.0, 1.0);
const PURPLE: (f64, f64, f64, f64) = (0.5, 0.0, 0.5, 1.0);

const SCALE: f64 = 15.0;
const SECTOR_HEX_WIDTH: i64 = 32;
const SECTOR_HEX_HEIGHT: i64 = 40;

struct HexInfo<'a> {
    hex: String,
    cx: f64,
    cy: f64,
    vertexes: Vec<(f64, f64)>,
    center: (f64, f64),
    coords_opt: Option<&'a Coords>,
}

fn get_hex_info(sector: &Sector, x: i64, y: i64) -> HexInfo {
    let hex = format!("{:02}{:02}", x, y);
    // leftmost point
    let cx = (4.0 + x as f64) * 3.0 * SCALE;
    // topmost point
    let cy = (3.0 + y as f64 * 2.0 + ((x as f64 - 1.0) as i64 & 1) as f64) * SQRT3 * SCALE;
    let vertexes = vec![
        (cx + SCALE, cy),
        (cx + 3.0 * SCALE, cy),
        (cx + 4.0 * SCALE, cy + SQRT3 * SCALE),
        (cx + 3.0 * SCALE, cy + 2.0 * SQRT3 * SCALE),
        (cx + SCALE, cy + 2.0 * SQRT3 * SCALE),
        (cx, cy + SQRT3 * SCALE),
    ];
    let center = (cx + 2.0 * SCALE, cy + SQRT3 * SCALE);
    let coords_opt = sector.hex_to_coords.get(&hex);
    HexInfo {
        hex,
        cx,
        cy,
        vertexes,
        center,
        coords_opt,
    }
}

fn draw_background(ctx: &Context, width: f64, height: f64) {
    let rgba = BLACK;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    ctx.rectangle(0.0, 0.0, width, height);
    ctx.fill().unwrap();
}

fn draw_sector_name(
    ctx: &Context,
    font_face: &FontFace,
    font_size: f64,
    name: &str,
    x_pos: f64,
    y_pos: f64,
) {
    // TODO Vertical text on the left and right sides would save space
    ctx.set_font_face(font_face);
    ctx.set_font_size(font_size);
    let rgba = WHITE;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    let extents = ctx.text_extents(name).unwrap();
    ctx.move_to(x_pos - extents.width / 2.0, y_pos - extents.height / 2.0);
    ctx.show_text(name).unwrap();
}

fn draw_sector_names(
    ctx: &Context,
    width: f64,
    height: f64,
    normal_font_face: &FontFace,
    bold_font_face: &FontFace,
    sector: &Sector,
    location_to_sector: &HashMap<(i64, i64), Sector>,
) {
    // This sector's name
    draw_sector_name(
        ctx,
        bold_font_face,
        3.0 * SCALE,
        &sector.name,
        width / SCALE / 4.0,
        6.0 * SCALE,
    );

    // neighboring sector names, if known

    // coreward (up)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0, sector.location.1 - 1))
    {
        draw_sector_name(
            ctx,
            normal_font_face,
            SCALE,
            &neighbor_sector.name,
            width / SCALE / 2.0,
            6.0 * SCALE,
        );
    }

    // spinward (left)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0 - 1, sector.location.1))
    {
        draw_sector_name(
            ctx,
            normal_font_face,
            SCALE,
            &neighbor_sector.name,
            5.0 * SCALE,
            height / SCALE / 2.0,
        );
    }

    // trailing (right)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0 + 1, sector.location.1))
    {
        draw_sector_name(
            ctx,
            normal_font_face,
            SCALE,
            &neighbor_sector.name,
            width / SCALE - 2.0 * SCALE,
            height / SCALE / 2.0,
        );
    }

    // rimward (down)
    if let Some(neighbor_sector) =
        location_to_sector.get(&(sector.location.0, sector.location.1 + 1))
    {
        draw_sector_name(
            ctx,
            normal_font_face,
            SCALE,
            &neighbor_sector.name,
            width / SCALE / 2.0,
            height / SCALE - 6.0 * SCALE,
        );
    }
}

fn draw_subsector_borders(ctx: &Context) {
    ctx.set_line_width(0.03 * SCALE);
    let rgba = GRAY;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);

    // vertical lines
    for x in &[1.0, 9.0, 17.0, 25.0, 33.0] {
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
    for y in &[1.0, 11.0, 21.0, 31.0, 41.0] {
        let x = 1.0;
        let cy = (3.0 + y * 2.0) * SQRT3 * SCALE;
        let cx1 = (25.0 / 6.0 + x) * 3.0 * SCALE;
        let x = 33.0;
        let cx2 = (25.0 / 6.0 + x) * 3.0 * SCALE;
        ctx.move_to(cx1, cy);
        ctx.line_to(cx2, cy);
        ctx.stroke().unwrap();
    }
}

fn draw_subsector_names(ctx: &Context, normal_font_face: &FontFace, sector: &Sector) {
    for row in 0..4 {
        for col in 0..4 {
            let letter = (char::from_u32(4 * row + col + u32::from('A'))).unwrap();
            if let Some(subsector_name) = sector.subsector_letter_to_name.get(&letter) {
                ctx.set_font_size(3.0 * SCALE);
                ctx.set_font_face(normal_font_face);
                let rgba = GRAY;
                ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
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
}

fn draw_hexsides(ctx: &Context, sector: &Sector) {
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let hexinfo = get_hex_info(sector, x, y);
            let vertexes = hexinfo.vertexes;
            ctx.set_line_width(0.03 * SCALE);
            ctx.move_to(vertexes[0].0, vertexes[0].1);
            let rgba = WHITE;
            ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
            for ii in &[1, 2, 3, 4, 5, 0] {
                ctx.line_to(vertexes[*ii].0, vertexes[*ii].1);
            }
            ctx.stroke().unwrap();
        }
    }
}

fn draw_route(
    ctx: &Context,
    coords1: Coords,
    coords_set: &HashSet<Coords>,
    line_width: f64,
    rgba: (f64, f64, f64, f64),
    (cx, cy): (f64, f64),
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

fn draw_xboat_routes(ctx: &Context, sector: &Sector, coords_to_world: &HashMap<Coords, World>) {
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let hexinfo = get_hex_info(sector, x, y);
            if let Some(coords) = hexinfo.coords_opt {
                if let Some(world) = coords_to_world.get(coords) {
                    draw_route(
                        ctx,
                        *coords,
                        &world.xboat_routes,
                        0.3 * SCALE,
                        PURPLE,
                        (hexinfo.cx, hexinfo.cy),
                        hexinfo.center,
                    );
                }
            }
        }
    }
}

fn draw_trade_routes(ctx: &Context, sector: &Sector, coords_to_world: &HashMap<Coords, World>) {
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let hexinfo = get_hex_info(sector, x, y);
            if let Some(coords) = hexinfo.coords_opt {
                let center = hexinfo.center;
                let cx = hexinfo.cx;
                let cy = hexinfo.cy;
                if let Some(world) = coords_to_world.get(coords) {
                    draw_route(
                        ctx,
                        *coords,
                        &world.major_routes,
                        0.09 * SCALE,
                        BLUE,
                        (cx, cy),
                        center,
                    );
                    draw_route(
                        ctx,
                        *coords,
                        &world.main_routes,
                        0.08 * SCALE,
                        CYAN,
                        (cx, cy),
                        center,
                    );
                    draw_route(
                        ctx,
                        *coords,
                        &world.intermediate_routes,
                        0.07 * SCALE,
                        GREEN,
                        (cx, cy),
                        center,
                    );
                    draw_route(
                        ctx,
                        *coords,
                        &world.feeder_routes,
                        0.06 * SCALE,
                        YELLOW,
                        (cx, cy),
                        center,
                    );
                    draw_route(
                        ctx,
                        *coords,
                        &world.minor_routes,
                        0.05 * SCALE,
                        RED,
                        (cx, cy),
                        center,
                    );
                }
            }
        }
    }
}

fn draw_uwp(ctx: &Context, font_face: &FontFace, world: &World, cx: f64, cy: f64) {
    ctx.set_font_size(0.35 * SCALE);
    ctx.set_font_face(font_face);
    let rgba = WHITE;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    let text = &world.uwp;
    let extents = ctx.text_extents(text).unwrap();
    ctx.move_to(
        cx + 2.0 * SCALE - extents.width / 2.0,
        cy + SQRT3 * SCALE * 1.5,
    );
    ctx.show_text(text).unwrap();
}

fn draw_world_name(ctx: &Context, font_face: &FontFace, world: &World, cx: f64, cy: f64) {
    // All-caps for high population
    let name: String = if world.population().is_alphabetic() || world.population() == '9' {
        world.name.to_owned().to_uppercase()
    } else {
        world.name.to_owned()
    };
    ctx.set_font_size(0.4 * SCALE);
    ctx.set_font_face(font_face);
    let extents = ctx.text_extents(&name).unwrap();
    // Red if a sector or subsector capital
    if world.trade_classifications.contains("Cp") || world.trade_classifications.contains("Cs") {
        let rgba = RED;
        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    } else {
        let rgba = WHITE;
        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    }
    ctx.move_to(
        cx + 2.0 * SCALE - extents.width / 2.0,
        cy + SQRT3 * SCALE * 1.75,
    );
    ctx.show_text(&name).unwrap();
}

/// Draw DWTN, endpoint trace BTN, transient trade BTN, and port size.
fn draw_trade_info(ctx: &Context, font_face: &FontFace, world: &World, cx: f64, cy: f64) {
    ctx.set_font_size(0.35 * SCALE);
    ctx.set_font_face(font_face);
    let rgba = WHITE;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    let dwtn = (world.wtn() * 2.0) as u64;
    let endpoint_dbtn = bisect_left(&DBTN_TO_CREDITS, &world.endpoint_trade_credits);
    let endpoint_btn = endpoint_dbtn / 2;
    let transient_dbtn = bisect_left(&DBTN_TO_CREDITS, &world.transient_trade_credits);
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
}

fn draw_world_circle(ctx: &Context, world: &World, center: (f64, f64)) {
    let mut rng = thread_rng();
    if world.size() == '0' {
        // Asteroid belt
        let rgba = WHITE;
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
        let rgba = WHITE;
        let mut fill_rgba = WHITE;
        if world.trade_classifications.contains("Ri") && world.trade_classifications.contains("Ag")
        {
            fill_rgba = YELLOW;
        } else if world.trade_classifications.contains("Ri") {
            fill_rgba = PURPLE;
        } else if world.trade_classifications.contains("Ag") {
            fill_rgba = GREEN;
        } else if world.atmosphere() == 'B' || world.atmosphere() == 'C' {
            fill_rgba = ORANGE;
        } else if world.atmosphere() == '0' {
            fill_rgba = BLACK;
        } else if world.hydrosphere() != '0' {
            fill_rgba = BLUE;
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
}

fn draw_gas_giant(ctx: &Context, world: &World, center: (f64, f64)) {
    if world.gas_giants() != '0' {
        let rgba = WHITE;
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
}

fn draw_zones(ctx: &Context, world: &World, center: (f64, f64)) {
    if world.zone == 'R' || world.zone == 'A' {
        let mut rgba = RED;
        if world.zone == 'A' {
            rgba = YELLOW;
        }
        ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        ctx.new_sub_path();
        ctx.arc(center.0, center.1, 1.5 * SCALE, 0.7 * PI, 2.3 * PI);
        ctx.set_line_width(0.03 * SCALE);
        ctx.stroke().unwrap();
    }
}

fn draw_hex_label(ctx: &Context, font_face: &FontFace, text: String, cx: f64, cy: f64) {
    ctx.set_font_size(0.35 * SCALE);
    ctx.set_font_face(font_face);
    let extents = ctx.text_extents(&text).unwrap();
    let rgba = WHITE;
    ctx.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    ctx.move_to(
        cx + 2.0 * SCALE - extents.width / 2.0,
        cy + SQRT3 * SCALE * 0.3,
    );
    ctx.show_text(&text).unwrap();
}

fn draw_worlds(
    ctx: &Context,
    normal_font_face: &FontFace,
    bold_font_face: &FontFace,
    sector: &Sector,
    coords_to_world: &HashMap<Coords, World>,
) {
    for x in 1..SECTOR_HEX_WIDTH + 1 {
        for y in 1..SECTOR_HEX_HEIGHT + 1 {
            let hexinfo = get_hex_info(sector, x, y);
            let cx = hexinfo.cx;
            let cy = hexinfo.cy;
            let center = hexinfo.center;
            if let Some(coords) = hexinfo.coords_opt {
                if let Some(world) = coords_to_world.get(coords) {
                    draw_uwp(ctx, normal_font_face, world, cx, cy);
                    draw_world_name(ctx, bold_font_face, world, cx, cy);
                    draw_trade_info(ctx, normal_font_face, world, cx, cy);
                    draw_world_circle(ctx, world, center);
                    draw_gas_giant(ctx, world, center);
                    draw_zones(ctx, world, center);
                }
                draw_hex_label(ctx, normal_font_face, hexinfo.hex, cx, cy);
            }
        }
    }
}

fn generate_pdf(
    sector: &Sector,
    output_dir: &Path,
    location_to_sector: &HashMap<(i64, i64), Sector>,
    coords_to_world: &HashMap<Coords, World>,
) {
    let width = 60.0 * SECTOR_HEX_WIDTH as f64 * SCALE;
    let height = 35.0 * SQRT3 * SECTOR_HEX_HEIGHT as f64 * SCALE;
    let output_filename = sector.name.to_owned() + ".pdf";
    let mut output_path = output_dir.to_path_buf();
    output_path.push(output_filename);

    let surface = PdfSurface::new(width, height, output_path).unwrap();
    let ctx = Context::new(&surface).unwrap();
    ctx.scale(SCALE, SCALE);

    draw_background(&ctx, width, height);

    let normal_font_face =
        FontFace::toy_create("Sans", FontSlant::Normal, FontWeight::Normal).unwrap();
    let bold_font_face = FontFace::toy_create("Sans", FontSlant::Normal, FontWeight::Bold).unwrap();

    draw_sector_names(
        &ctx,
        width,
        height,
        &normal_font_face,
        &bold_font_face,
        sector,
        location_to_sector,
    );
    draw_subsector_borders(&ctx);
    draw_subsector_names(&ctx, &normal_font_face, sector);
    draw_hexsides(&ctx, sector);
    draw_xboat_routes(&ctx, sector, coords_to_world);
    draw_trade_routes(&ctx, sector, coords_to_world);
    draw_worlds(
        &ctx,
        &normal_font_face,
        &bold_font_face,
        sector,
        coords_to_world,
    );

    surface.finish();
}

pub fn generate_pdfs(
    output_dir: &Path,
    location_to_sector: &HashMap<(i64, i64), Sector>,
    coords_to_world: &HashMap<Coords, World>,
) {
    debug!("(parallel) generate_pdfs");
    location_to_sector
        .par_iter()
        .map(|(_, sector)| generate_pdf(sector, output_dir, location_to_sector, coords_to_world))
        .collect::<Vec<()>>();
}
