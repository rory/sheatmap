extern crate rstar;
extern crate csv;
extern crate clap;
extern crate anyhow;
#[macro_use] extern crate log;
extern crate env_logger;
use clap::{Arg, App};
use std::io::{Write, BufWriter};
use std::fs::*;

use anyhow::{Context, Result};

static EARTH_RADIUS_M: f64 = 6_371_000.;


fn main() -> Result<()> {
    let matches = App::new("sHeatMap")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Create heatmaps")
        .arg(Arg::with_name("input")
             .short("i").long("input")
             .help("Input CSV file").value_name("INPUT.csv")
             .takes_value(true).required(true))
        .arg(Arg::with_name("output")
             .short("o").long("output")
             .help("Output XYZ ASCII grid file")
             .value_name("OUTPUT.xyz")
             .takes_value(true).required(true))
        .arg(Arg::with_name("xmin")
             .long("xmin")
             .takes_value(true))
        .arg(Arg::with_name("xmax")
             .long("xmax")
             .takes_value(true))
        .arg(Arg::with_name("ymin")
             .long("ymin")
             .takes_value(true))
        .arg(Arg::with_name("ymax")
             .long("ymax")
             .takes_value(true))
        .arg(Arg::with_name("res")
             .short("R").long("res")
             .help("Resolution of image")
             .value_name("xrex yres")
             .number_of_values(2)
             .takes_value(true).required(true))
        .arg(Arg::with_name("size")
             .short("s").long("size")
             .help("Size of image")
             .value_name("width height")
             .number_of_values(2)
             .takes_value(true))
        .arg(Arg::with_name("radius")
             .short("r").long("radius")
             .help("Radius value for heatmap")
             .takes_value(true).required(true)
             .default_value("10")
             )
        .arg(Arg::with_name("assume_lat_lon")
             .long("assume-lat-lon")
             )
        .get_matches();

    env_logger::init();

    let mut points = vec![];
    let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_path(matches.value_of("input").unwrap())?;
    info!("Reading points from {}", matches.value_of("input").unwrap());
    let mut xmax = None;
    let mut xmin = None;
    let mut ymax = None;
    let mut ymin = None;
    for result in csv_reader.records() {
        let record = result?;
        let x: f64 = record.get(0).context("getting x")?.parse()?;
        let y: f64 = record.get(1).context("getting y")?.parse()?;
        xmax = xmax.map(|xmax| if x > xmax { Some(x) } else { Some(xmax) }).unwrap_or(Some(x));
        xmin = xmin.map(|xmin| if x < xmin { Some(x) } else { Some(xmin) }).unwrap_or(Some(x));
        ymax = ymax.map(|ymax| if y > ymax { Some(y) } else { Some(ymax) }).unwrap_or(Some(y));
        ymin = ymin.map(|ymin| if y < ymin { Some(y) } else { Some(ymin) }).unwrap_or(Some(y));
        points.push([x, y]);
    }
    info!("Read in {} points", points.len());
    let tree = rstar::RTree::bulk_load(points);
    let assume_lat_lon = matches.is_present("assume_lat_lon");

    let radius: f64 = matches.value_of("radius").unwrap().parse()?;
    let radius_sq = radius.powi(2);

    // used for bbox query
    let approx_radius_deg = to_srs_coord(assume_lat_lon, radius);

    let xmin: f64 = match matches.value_of("xmin") { None => xmin.unwrap()-approx_radius_deg, Some(xmin) => xmin.parse()? };
    let xmax: f64 = match matches.value_of("xmax") { None => xmax.unwrap()+approx_radius_deg, Some(xmax) => xmax.parse()? };
    let ymin: f64 = match matches.value_of("ymin") { None => ymin.unwrap()-approx_radius_deg, Some(ymin) => ymin.parse()? };
    let ymax: f64 = match matches.value_of("ymax") { None => ymax.unwrap()+approx_radius_deg, Some(ymax) => ymax.parse()? };


    let xres: f64 = to_srs_coord(assume_lat_lon, matches.values_of("res").unwrap().nth(0).unwrap().parse()?);
    let yres: f64 = to_srs_coord(assume_lat_lon, matches.values_of("res").unwrap().nth(1).unwrap().parse()?);


    let width = ((xmax - xmin)/xres).round() as usize;
    let height = ((ymax - ymin)/yres).round() as usize;

    let output_path = matches.value_of("output").unwrap();
    let mut output = BufWriter::new(File::create(output_path)?);
    writeln!(output, "x y z")?;

    let mut value;
    let mut posy; let mut posx;


    for j in 0..height {
        if j % 100 == 0 {
            info!("{} of {} done", j, height);
        }
        posy = ymin + (j as f64) * yres;
        for i in 0..width {
            posx = xmin + (i as f64) * xres;

            value = 0.;
            for [x, y] in tree.locate_in_envelope(&rstar::AABB::from_corners([posx-approx_radius_deg, posy-approx_radius_deg], [posx+approx_radius_deg, posy+approx_radius_deg])) {
                if assume_lat_lon {
                    let dist = haversine_dist(*y, *x, posy, posx);
                    if dist <= radius {
                        value += (15./16.)*(1. - (dist/radius).powi(2)).powi(2);
                    }
                } else {
                    let dist_sq = (x-posx).powi(2) + (y-posy).powi(2);
                    if dist_sq <= radius_sq {
                        value += (15./16.)*(1. - (dist_sq.sqrt()/radius).powi(2)).powi(2);
                    }
                }
            }

            writeln!(output, "{} {} {}", posx, posy, value)?;
        }
    }
    info!("finished");

    Ok(())
}

fn to_srs_coord(assume_lat_lon: bool, val: f64) -> f64 {
    if assume_lat_lon {
        val / 110_000.
    } else {
        val
    }
}

fn haversine_dist(mut th1: f64, mut ph1: f64, mut th2: f64, ph2: f64) -> f64 {
    ph1 -= ph2;
    ph1 = ph1.to_radians();
    th1 = th1.to_radians();
    th2 = th2.to_radians();
    let dz: f64 = th1.sin() - th2.sin();
    let dx: f64 = ph1.cos() * th1.cos() - th2.cos();
    let dy: f64 = ph1.sin() * th1.cos();
    ((dx * dx + dy * dy + dz * dz).sqrt() / 2.0).asin() * 2.0 * EARTH_RADIUS_M
}


