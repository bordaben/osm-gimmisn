/*
 * Copyright 2021 Miklos Vajna. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#![deny(warnings)]
#![warn(clippy::all)]
#![warn(missing_docs)]

//! The cron module allows doing nightly tasks.

use crate::areas;
use crate::context;
use crate::overpass_query;
use crate::stats;
use crate::util;
use anyhow::Context;
use chrono::Datelike;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufRead;
use std::io::Write;
use std::ops::DerefMut;

#[cfg(not(test))]
use log::{error, info, warn};

#[cfg(test)]
use std::{println as info, println as warn, println as error};

/// Sleeps to respect overpass rate limit.
fn overpass_sleep(ctx: &context::Context) {
    loop {
        let sleep = overpass_query::overpass_query_need_sleep(ctx);
        if sleep == 0 {
            break;
        }
        info!("overpass_sleep: waiting for {} seconds", sleep);
        ctx.get_time().sleep(sleep as u64);
    }
}

/// Decides if we should retry a query or not.
fn should_retry(retry: i32) -> bool {
    retry < 20
}

/// Update the OSM street list of all relations.
fn update_osm_streets(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    let active_names = relations.get_active_names();
    for relation_name in active_names.context("get_active_names() failed")? {
        let relation = relations.get_relation(&relation_name)?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_osm_streets_path())
        {
            continue;
        }
        info!("update_osm_streets: start: {}", relation_name);
        let mut retry = 0;
        while should_retry(retry) {
            if retry > 0 {
                info!("update_osm_streets: try #{}", retry);
            }
            retry += 1;
            overpass_sleep(ctx);
            let query = relation.get_osm_streets_query()?;
            let buf = match overpass_query::overpass_query(ctx, query) {
                Ok(value) => value,
                Err(err) => {
                    info!("update_osm_streets: http error: {:?}", err);
                    continue;
                }
            };
            if relation.get_files().write_osm_streets(ctx, &buf)? == 0 {
                info!("update_osm_streets: short write");
                continue;
            }
            break;
        }
        info!("update_osm_streets: end: {}", relation_name);
    }

    Ok(())
}

/// Update the OSM housenumber list of all relations.
fn update_osm_housenumbers(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    for relation_name in relations.get_active_names()? {
        let relation = relations.get_relation(&relation_name)?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_osm_housenumbers_path())
        {
            continue;
        }
        info!("update_osm_housenumbers: start: {}", relation_name);
        let mut retry = 0;
        while should_retry(retry) {
            if retry > 0 {
                info!("update_osm_housenumbers: try #{}", retry);
            }
            retry += 1;
            overpass_sleep(ctx);
            let query = relation.get_osm_housenumbers_query()?;
            let buf = match overpass_query::overpass_query(ctx, query) {
                Ok(value) => value,
                Err(err) => {
                    info!("update_osm_housenumbers: http error: {:?}", err);
                    continue;
                }
            };
            if relation.get_files().write_osm_housenumbers(ctx, &buf)? == 0 {
                info!("update_osm_housenumbers: short write");
                continue;
            }
            break;
        }
        info!("update_osm_housenumbers: end: {}", relation_name);
    }

    Ok(())
}

/// Update the reference housenumber list of all relations.
fn update_ref_housenumbers(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    for relation_name in relations.get_active_names()? {
        let relation = relations.get_relation(&relation_name)?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_ref_housenumbers_path())
        {
            continue;
        }
        let references = ctx.get_ini().get_reference_housenumber_paths()?;
        let streets = relation.get_config().should_check_missing_streets();
        if streets == "only" {
            continue;
        }

        info!("update_ref_housenumbers: start: {}", relation_name);
        if let Err(err) = relation.write_ref_housenumbers(&references) {
            info!("update_osm_housenumbers: failed: {:?}", err);
            continue;
        }
        info!("update_ref_housenumbers: end: {}", relation_name);
    }

    Ok(())
}

/// Update the reference street list of all relations.
fn update_ref_streets(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    for relation_name in relations.get_active_names()? {
        let relation = relations.get_relation(&relation_name)?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_ref_streets_path())
        {
            continue;
        }
        let reference = ctx.get_ini().get_reference_street_path()?;
        let streets = relation.get_config().should_check_missing_streets();
        if streets == "no" {
            continue;
        }

        info!("update_ref_streets: start: {}", relation_name);
        relation.write_ref_streets(&reference)?;
        info!("update_ref_streets: end: {}", relation_name);
    }

    Ok(())
}

/// Update the relation's house number coverage stats.
fn update_missing_housenumbers(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    info!("update_missing_housenumbers: start");
    let active_names = relations
        .get_active_names()
        .context("get_active_names() failed")?;
    for relation_name in active_names {
        let mut relation = relations
            .get_relation(&relation_name)
            .context("get_relation() failed")?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_housenumbers_percent_path())
        {
            continue;
        }
        let streets = relation.get_config().should_check_missing_streets();
        if streets == "only" {
            continue;
        }

        relation
            .write_missing_housenumbers()
            .context("write_missing_housenumbers() failed")?;
    }
    info!("update_missing_housenumbers: end");

    Ok(())
}

/// Update the relation's street coverage stats.
fn update_missing_streets(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    info!("update_missing_streets: start");
    for relation_name in relations.get_active_names()? {
        let relation = relations.get_relation(&relation_name)?;
        if !update
            && ctx
                .get_file_system()
                .path_exists(&relation.get_files().get_streets_percent_path())
        {
            continue;
        }
        let streets = relation.get_config().should_check_missing_streets();
        if streets == "no" {
            continue;
        }

        relation.write_missing_streets()?;
    }
    info!("update_missing_streets: end");

    Ok(())
}

/// Update the relation's "additional streets" stats.
fn update_additional_streets(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    update: bool,
) -> anyhow::Result<()> {
    info!("update_additional_streets: start");
    for relation_name in relations.get_active_names()? {
        let relation = relations.get_relation(&relation_name)?;
        let relation_path = relation.get_files().get_streets_additional_count_path();
        if !update && ctx.get_file_system().path_exists(&relation_path) {
            continue;
        }
        let streets = relation.get_config().should_check_missing_streets();
        if streets == "no" {
            continue;
        }

        relation.write_additional_streets()?;
    }
    info!("update_additional_streets: end");

    Ok(())
}

/// Writes a daily .citycount file.
fn write_city_count_path(
    ctx: &context::Context,
    city_count_path: &str,
    cities: &HashMap<String, HashSet<String>>,
) -> anyhow::Result<()> {
    let stream = ctx.get_file_system().open_write(city_count_path)?;
    let mut guard = stream.borrow_mut();
    let mut cities: Vec<_> = cities.iter().map(|(key, value)| (key, value)).collect();
    cities.sort_by_key(|(key, _value)| util::get_sort_key(key).unwrap());
    cities.dedup();
    // Locale-aware sort, by key.
    for (key, value) in cities {
        let line = format!("{}\t{}\n", key, value.len());
        guard.write_all(line.as_bytes())?;
    }

    Ok(())
}

/// Writes a daily .zipcount file.
fn write_zip_count_path(
    ctx: &context::Context,
    zip_count_path: &str,
    zips: &HashMap<String, HashSet<String>>,
) -> anyhow::Result<()> {
    let stream = ctx.get_file_system().open_write(zip_count_path)?;
    let mut guard = stream.borrow_mut();
    let mut zips: Vec<_> = zips.iter().map(|(key, value)| (key, value)).collect();

    zips.sort_by_key(|(key, _value)| key.to_string());
    zips.dedup();
    for (key, value) in zips {
        let key = if key.is_empty() { "_Empty" } else { key };
        let line = format!("{}\t{}\n", key, value.len());
        guard.write_all(line.as_bytes())?;
    }

    Ok(())
}

/// Counts the # of all house numbers as of today.
fn update_stats_count(ctx: &context::Context, today: &str) -> anyhow::Result<()> {
    let statedir = ctx.get_abspath("workdir/stats");
    let csv_path = format!("{}/{}.csv", statedir, today);
    if !ctx.get_file_system().path_exists(&csv_path) {
        return Ok(());
    }
    let count_path = format!("{}/{}.count", statedir, today);
    let city_count_path = format!("{}/{}.citycount", statedir, today);
    let zip_count_path = format!("{}/{}.zipcount", statedir, today);
    let mut house_numbers: HashSet<String> = HashSet::new();
    let mut cities: HashMap<String, HashSet<String>> = HashMap::new();
    let mut zips: HashMap<String, HashSet<String>> = HashMap::new();
    let mut first = true;
    let valid_settlements =
        util::get_valid_settlements(ctx).context("get_valid_settlements() failed")?;
    let stream = ctx.get_file_system().open_read(&csv_path)?;
    let mut guard = stream.borrow_mut();
    let mut read = std::io::BufReader::new(guard.deref_mut());
    let mut csv_read = util::CsvRead::new(&mut read);
    let mut columns: HashMap<String, usize> = HashMap::new();
    for result in csv_read.records() {
        let row = result?;
        if !row.is_empty() && row[0].starts_with("<?xml") {
            // Not a CSV, reject.
            break;
        }
        if first {
            first = false;
            for (index, label) in row.iter().enumerate() {
                columns.insert(label.into(), index);
            }
            continue;
        }
        let post_code = &row[columns["addr:postcode"]];
        let city = &row[columns["addr:city"]];
        let street = &row[columns["addr:street"]];
        let house_number = &row[columns["addr:housenumber"]];
        // This ignores the @user column.
        house_numbers.insert([post_code, city, street, house_number].join("\t"));
        let city_key = util::get_city_key(post_code, city, &valid_settlements)
            .context("get_city_key() failed")?;
        let city_value = [street, house_number].join("\t");
        let entry = cities.entry(city_key).or_insert_with(HashSet::new);
        entry.insert(city_value);

        // Postcode.
        let zip_key = post_code.to_string();
        // Street name and housenumber.
        let zip_value = [street, house_number].join("\t");
        let zip_entry = zips.entry(zip_key).or_insert_with(HashSet::new);
        zip_entry.insert(zip_value);
    }
    ctx.get_file_system()
        .write_from_string(&house_numbers.len().to_string(), &count_path)?;
    write_city_count_path(ctx, &city_count_path, &cities)
        .context("write_city_count_path() failed")?;
    write_zip_count_path(ctx, &zip_count_path, &zips).context("write_zip_count_path() failed")
}

/// Counts the top housenumber editors as of today.
fn update_stats_topusers(ctx: &context::Context, today: &str) -> anyhow::Result<()> {
    let statedir = ctx.get_abspath("workdir/stats");
    let csv_path = format!("{}/{}.csv", statedir, today);
    if !ctx.get_file_system().path_exists(&csv_path) {
        return Ok(());
    }
    let topusers_path = format!("{}/{}.topusers", statedir, today);
    let usercount_path = format!("{}/{}.usercount", statedir, today);
    let mut users: HashMap<String, u64> = HashMap::new();
    {
        let stream = ctx.get_file_system().open_read(&csv_path)?;
        let mut guard = stream.borrow_mut();
        let mut read = std::io::BufReader::new(guard.deref_mut());
        let mut csv_read = util::CsvRead::new(&mut read);
        let mut columns: HashMap<String, usize> = HashMap::new();
        let mut first = true;
        for result in csv_read.records() {
            let row = result?;
            if first {
                first = false;
                for (index, label) in row.iter().enumerate() {
                    columns.insert(label.into(), index);
                }
                continue;
            }
            // Only care about the last column.
            let user = row[columns["@user"]].to_string();
            let entry = users.entry(user).or_insert(0);
            (*entry) += 1;
        }
    }
    {
        let stream = ctx.get_file_system().open_write(&topusers_path)?;
        let mut guard = stream.borrow_mut();
        let mut users: Vec<_> = users.iter().map(|(key, value)| (key, value)).collect();
        users.sort_by_key(|i| Reverse(i.1));
        users.dedup();
        users = users[0..std::cmp::min(20, users.len())].to_vec();
        for user in users {
            let line = format!("{} {}\n", user.1, user.0);
            guard.write_all(line.as_bytes())?;
        }
    }

    let line = format!("{}\n", users.len());
    ctx.get_file_system()
        .write_from_string(&line, &usercount_path)
}

/// Performs the update of workdir/stats/ref.count.
fn update_stats_refcount(ctx: &context::Context, state_dir: &str) -> anyhow::Result<()> {
    let mut count = 0;
    {
        let stream = ctx
            .get_file_system()
            .open_read(&ctx.get_ini().get_reference_citycounts_path()?)?;
        let mut guard = stream.borrow_mut();
        let mut read = guard.deref_mut();
        let mut csv_read = util::CsvRead::new(&mut read);
        let mut first = true;
        for result in csv_read.records() {
            let row = result?;
            if first {
                first = false;
                continue;
            }

            count += row[1].parse::<i32>()?;
        }
    }

    let string = format!("{}\n", count);
    let path = format!("{}/ref.count", state_dir);
    ctx.get_file_system().write_from_string(&string, &path)
}

/// Performs the update of country-level stats.
fn update_stats(ctx: &context::Context, overpass: bool) -> anyhow::Result<()> {
    // Fetch house numbers for the whole country.
    info!("update_stats: start, updating whole-country csv");
    let query = ctx
        .get_file_system()
        .read_to_string(&ctx.get_abspath("data/street-housenumbers-hungary.txt"))?;
    let statedir = ctx.get_abspath("workdir/stats");
    let now = chrono::NaiveDateTime::from_timestamp(ctx.get_time().now(), 0);
    let today = now.format("%Y-%m-%d").to_string();
    let csv_path = format!("{}/{}.csv", statedir, today);

    if overpass {
        info!("update_stats: talking to overpass");
        let mut retry = 0;
        while should_retry(retry) {
            if retry > 0 {
                info!("update_stats: try #{}", retry);
            }
            retry += 1;
            overpass_sleep(ctx);
            let response = match overpass_query::overpass_query(ctx, query.clone()) {
                Ok(value) => value,
                Err(err) => {
                    info!("update_stats: http error: {}", err);
                    continue;
                }
            };
            ctx.get_file_system()
                .write_from_string(&response, &csv_path)?;
            break;
        }
    }

    update_stats_count(ctx, &today).context("update_stats_count() failed")?;
    update_stats_topusers(ctx, &today)?;
    update_stats_refcount(ctx, &statedir)?;

    // Remove old CSV files as they are created daily and each is around 11M.
    for file_name in ctx.get_file_system().listdir(&statedir)? {
        if !file_name.ends_with("csv") {
            continue;
        }

        let last_modified =
            ctx.get_time().now() as f64 - ctx.get_file_system().getmtime(&file_name)?;

        if last_modified >= 24_f64 * 3600_f64 * 7_f64 {
            ctx.get_file_system().unlink(&file_name)?;
            info!("update_stats: removed old {}", file_name);
        }
    }

    info!("update_stats: generating json");
    let json_path = format!("{}/stats.json", &statedir);
    stats::generate_json(ctx, &statedir, &json_path)?;

    info!("update_stats: end");

    Ok(())
}

/// Performs the actual nightly task.
fn our_main_inner(
    ctx: &context::Context,
    relations: &mut areas::Relations,
    mode: &String,
    update: bool,
    overpass: bool,
) -> anyhow::Result<()> {
    if mode == "all" || mode == "stats" {
        update_stats(ctx, overpass)?;
    }
    if mode == "all" || mode == "relations" {
        update_osm_streets(ctx, relations, update)?;
        update_osm_housenumbers(ctx, relations, update)?;
        update_ref_streets(ctx, relations, update)?;
        update_ref_housenumbers(ctx, relations, update)?;
        update_missing_streets(ctx, relations, update)?;
        update_missing_housenumbers(ctx, relations, update)?;
        update_additional_streets(ctx, relations, update)?;
    }

    let pid = std::process::id();
    let stream = std::fs::File::open(format!("/proc/{}/status", pid))?;
    let reader = std::io::BufReader::new(stream);
    for line in reader.lines() {
        let line = line?.to_string();
        if line.starts_with("VmPeak:") {
            let vm_peak = line.trim();
            info!("our_main: {}", vm_peak);
            break;
        }
    }

    ctx.get_unit().make_error()
}

/// Inner main() that is allowed to fail.
pub fn our_main(
    argv: &[String],
    _stream: &mut dyn Write,
    ctx: &context::Context,
) -> anyhow::Result<()> {
    let mut relations = areas::Relations::new(ctx)?;

    let refcounty = clap::Arg::new("refcounty")
        .long("refcounty")
        .help("limit the list of relations to a given refcounty");
    let refsettlement = clap::Arg::new("refsettlement")
        .long("refsettlement")
        .help("limit the list of relations to a given refsettlement");
    // Default: true.
    let no_update = clap::Arg::new("no-update")
        .long("no-update")
        .action(clap::ArgAction::SetTrue)
        .help("don't update existing state of relations");
    let mode = clap::Arg::new("mode")
        .long("mode")
        .default_value("relations")
        .help("only perform the given sub-task or all of them [all, stats or relations]");
    let no_overpass = clap::Arg::new("no-overpass") // default: true
        .long("no-overpass")
        .action(clap::ArgAction::SetTrue)
        .help("when updating stats, don't perform any overpass update");
    let args = [refcounty, refsettlement, no_update, mode, no_overpass];
    let app = clap::Command::new("osm-gimmisn");
    let args = app.args(&args).try_get_matches_from(argv)?;

    let start = ctx.get_time().now();
    // Query inactive relations once a month.
    let now = chrono::NaiveDateTime::from_timestamp(start, 0);
    let first_day_of_month = now.date().day() == 1;
    relations.activate_all(ctx.get_ini().get_cron_update_inactive() || first_day_of_month);
    relations.activate_new();
    let refcounty: Option<&String> = args.get_one("refcounty");
    relations.limit_to_refcounty(&refcounty)?;
    // Use map(), which handles optional values.
    let refsettlement: Option<&String> = args.get_one("refsettlement");
    relations.limit_to_refsettlement(&refsettlement)?;
    let update = !args.get_one::<bool>("no-update").unwrap();
    let overpass = !args.get_one::<bool>("no-overpass").unwrap();
    our_main_inner(
        ctx,
        &mut relations,
        args.get_one("mode").unwrap(),
        update,
        overpass,
    )?;
    let duration = chrono::Duration::seconds(ctx.get_time().now() - start);
    let seconds = duration.num_seconds() % 60;
    let minutes = duration.num_minutes() % 60;
    let hours = duration.num_hours();
    let duration = format!("{}:{:0>2}:{:0>2}", hours, minutes, seconds);
    info!("main: finished in {}", duration,);

    Ok(())
}

/// Similar to plain main(), but with an interface that allows testing.
pub fn main(argv: &[String], stream: &mut dyn Write, ctx: &context::Context) -> i32 {
    match our_main(argv, stream, ctx) {
        Ok(_) => 0,
        Err(err) => {
            error!("main: unhandled error: {:?}", err);
            1
        }
    }
}

#[cfg(test)]
mod tests;
