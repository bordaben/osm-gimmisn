/*
 * Copyright 2021 Miklos Vajna
 *
 * SPDX-License-Identifier: MIT
 */

#![deny(warnings)]
#![warn(clippy::all)]
#![warn(missing_docs)]

//! The cache module accelerates some functions of the areas module.

use crate::areas;
use crate::context;
use crate::stats;
use anyhow::Context as _;

/// Decides if we have an up to date cache entry or not.
fn is_sql_cache_current(
    ctx: &context::Context,
    cache_key: &str,
    dependencies: &[String],
    sql_dependencies: &[String],
) -> anyhow::Result<bool> {
    if !stats::has_sql_mtime(ctx, cache_key)? {
        return Ok(false);
    }

    let cache_mtime = stats::get_sql_mtime(ctx, cache_key)?;

    for dependency in dependencies {
        if ctx.get_file_system().path_exists(dependency)
            && ctx
                .get_file_system()
                .getmtime(dependency)
                .context("getmtime() failed")?
                > cache_mtime
        {
            return Ok(false);
        }
    }

    for dependency in sql_dependencies {
        if stats::has_sql_mtime(ctx, dependency)?
            && stats::get_sql_mtime(ctx, dependency)? > cache_mtime
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Decides if we have an up to date json cache entry or not.
fn is_missing_housenumbers_json_cached(relation: &mut areas::Relation<'_>) -> anyhow::Result<bool> {
    let datadir = relation.get_ctx().get_abspath("data");
    let relation_path = format!("{}/relation-{}.yaml", datadir, relation.get_name());
    let dependencies = vec![relation_path];
    let sql_dependencies = vec![
        format!("streets/{}", relation.get_name()),
        format!("housenumbers/{}", relation.get_name()),
    ];
    is_sql_cache_current(
        relation.get_ctx(),
        &format!("missing-housenumbers-cache/{}", relation.get_name()),
        &dependencies,
        &sql_dependencies,
    )
    .context("is_sql_cache_current() failed")
}

/// Gets the cached json of the missing housenumbers for a relation.
pub fn get_missing_housenumbers_json(relation: &mut areas::Relation<'_>) -> anyhow::Result<String> {
    let output: String;
    if is_missing_housenumbers_json_cached(relation)
        .context("is_missing_housenumbers_json_cached() failed")?
    {
        output = stats::get_sql_json(
            relation.get_ctx(),
            "missing_housenumbers_cache",
            &relation.get_name(),
        )?;
        return Ok(output);
    }

    let missing_housenumbers = relation.get_missing_housenumbers()?;
    output = serde_json::to_string(&missing_housenumbers)?;

    stats::set_sql_json(
        relation.get_ctx(),
        "missing_housenumbers_cache",
        &relation.get_name(),
        &output,
    )?;
    stats::set_sql_mtime(
        relation.get_ctx(),
        &format!("missing-housenumbers-cache/{}", &relation.get_name()),
    )?;

    relation.write_lints()?;

    Ok(output)
}

/// Decides if we have an up to date additional json cache entry or not.
fn is_additional_housenumbers_json_cached(
    relation: &mut areas::Relation<'_>,
) -> anyhow::Result<bool> {
    let datadir = relation.get_ctx().get_abspath("data");
    let relation_path = format!("{}/relation-{}.yaml", datadir, relation.get_name());
    let dependencies = vec![relation_path];
    let sql_dependencies = vec![
        format!("streets/{}", relation.get_name()),
        format!("housenumbers/{}", relation.get_name()),
    ];
    is_sql_cache_current(
        relation.get_ctx(),
        &format!("additional-housenumbers-cache/{}", relation.get_name()),
        &dependencies,
        &sql_dependencies,
    )
}

/// Gets the cached json of the additional housenumbers for a relation.
pub fn get_additional_housenumbers_json(
    relation: &mut areas::Relation<'_>,
) -> anyhow::Result<String> {
    let output: String;
    if is_additional_housenumbers_json_cached(relation)? {
        output = stats::get_sql_json(
            relation.get_ctx(),
            "additional_housenumbers_cache",
            &relation.get_name(),
        )?;
        return Ok(output);
    }

    let additional_housenumbers = relation.get_additional_housenumbers()?;
    output = serde_json::to_string(&additional_housenumbers)?;

    stats::set_sql_json(
        relation.get_ctx(),
        "additional_housenumbers_cache",
        &relation.get_name(),
        &output,
    )?;
    stats::set_sql_mtime(
        relation.get_ctx(),
        &format!("additional-housenumbers-cache/{}", &relation.get_name()),
    )?;
    Ok(output)
}

#[cfg(test)]
mod tests;
