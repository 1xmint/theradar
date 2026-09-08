// SPDX-License-Identifier: Apache-2.0
//! The planted tests for the walk-forward protocol.
//!
//! # Why this file exists
//!
//! A harness that never says `Found` passes every test about not being fooled,
//! and is worthless. A harness that says `Found` about noise is worse than
//! worthless, because the next document written from it argues for putting real
//! money behind it. Both failures are here, in that order:
//!
//! - `noise_is_never_found_across_ten_seeds` plants a feature that is uniform
//!   noise by construction. It will often win the fitting period — with
//!   thousands of strata enumerated, something always does — and it must fail
//!   the test folds every time.
//! - `an_engineered_edge_is_found` plants an edge large enough that missing it
//!   would mean the protocol cannot see one at all.
//!
//! The snapshot is the real one, read from the repository, because the bar and
//! the round trip must come from it rather than from a number written here.

use radar_research::edge::{self, Enumeration, Horizon, Options};
use radar_research::features::{FEATURES, FeatureTable, Row};
use radar_roast::BaseRates;
use radar_types::{Address, Slot};

/// Rows in a synthetic table.
///
/// Five folds of at least a hundred, after a purge and an embargo that each
/// remove a fold's worth of rows near a boundary.
const ROWS: usize = 3_000;

/// Slots between one synthetic launch and the next.
///
/// Wide enough that the twenty-four-hour embargo removes a fifth of a test
/// fold rather than all of it — which is the shape of the real store, where
/// the folds are weeks and the embargo is a day.
const SPACING: u64 = 1_000;

fn rates() -> BaseRates {
    BaseRates::load("../../docs/research/data/0024-base-rates.json")
        .expect("the repository's own snapshot")
}

/// A deterministic pseudo-random value in `[0, 1)` from an index.
///
/// Not a good generator, and it does not need to be: it needs to be the same
/// on every run and uncorrelated with the feature the test plants.
fn pseudo(index: usize, salt: u64) -> f64 {
    let mut x = (index as u64).wrapping_mul(6_364_136_223_846_793_005) ^ salt;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    #[expect(
        clippy::cast_precision_loss,
        reason = "masked to a million, exact in f64"
    )]
    let out = (x % 1_000_000) as f64 / 1_000_000.0;
    out
}

fn address(index: usize) -> Address {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    Address::new(bytes)
}

/// Builds a table whose labels come from `label`, given the row's index and its
/// three planted feature values.
fn table_of(label: impl Fn(usize, [f64; 3]) -> f64) -> FeatureTable {
    let rows = (0..ROWS)
        .map(|index| {
            let planted = [
                pseudo(index, 11) * 100.0,
                pseudo(index, 22) * 100.0,
                pseudo(index, 33) * 100.0,
            ];
            let mut values = vec![None; FEATURES.len()];
            for (slot, value) in values.iter_mut().take(3).zip(planted) {
                *slot = Some(value);
            }
            let gross = label(index, planted);
            Row {
                mint: address(index),
                creator: address(index % 50),
                launch_slot: Slot(index as u64 * SPACING),
                t: Slot(index as u64 * SPACING + 6_000),
                values,
                gross_6h_bps: Some(gross),
                gross_24h_bps: Some(gross),
                mode: None,
            }
        })
        .collect();

    FeatureTable {
        watermark: Slot(ROWS as u64 * SPACING),
        entry_offset: radar_research::features::ENTRY_OFFSET_SLOTS,
        rows,
    }
}

#[test]
fn noise_is_never_found_across_ten_seeds() {
    // Overfitting dies on fold two, and this is what proves the fold design
    // does its half of the job. Labels are drawn independently of every
    // feature, so there is nothing to find; the planted noise feature is one
    // more thing to find it in.
    let rates = rates();
    let table = table_of(|index, _| (pseudo(index, 99) - 0.5) * 4_000.0);

    for seed in 0..10u64 {
        let report = edge::run(
            &table,
            &rates,
            &Options {
                horizon: Horizon::TwentyFourHours,
                noise_seed: Some(seed),
                ..Options::default()
            },
        )
        .expect("the protocol runs");

        assert!(
            !report.found,
            "seed {seed} reported an edge in noise: {:?}",
            report.fitted.as_ref().map(|c| &c.stratum.name)
        );
        assert!(
            report.strata_tried > 0,
            "seed {seed} tried nothing, so it proved nothing"
        );
        assert_eq!(
            report.enumeration,
            Enumeration::Exhaustive,
            "seed {seed} did not finish the grammar, so its null is weaker than it looks"
        );
        // The fitting period always has a winner when there are enough rows —
        // that is what maximising over thousands of strata does — and the point
        // is that the winner does not survive.
        let fitted = report.fitted.expect("a fitting period always has a winner");
        assert!(
            !fitted.found,
            "seed {seed}: {} survived both test folds",
            fitted.stratum.name
        );
    }
}

#[test]
fn an_engineered_edge_is_found() {
    // The other failure, and the more dangerous one to leave untested: a
    // harness that says `NotFound` unconditionally passes every test above.
    // Feature 0 above fifty pays well over the bar, consistently, in every
    // window; if the protocol cannot see that it cannot see anything.
    let rates = rates();
    let table = table_of(|index, planted| {
        if planted[0] >= 50.0 {
            3_000.0 + pseudo(index, 7) * 200.0
        } else {
            -500.0 + pseudo(index, 8) * 200.0
        }
    });

    let report = edge::run(&table, &rates, &Options::default()).expect("the protocol runs");

    assert!(report.found, "a 3,000 bps edge went unnoticed");
    let fitted = report.fitted.expect("a winner");
    assert!(fitted.found, "the winner did not survive its test folds");
    assert!(
        fitted.stratum.name.contains(FEATURES[0]),
        "the winner should name the feature that carries the edge: {}",
        fitted.stratum.name
    );
    for reading in fitted.tests.iter().flatten() {
        assert!(
            reading.median_net > reading.se_median,
            "a surviving fold pays for its round trip by more than its own noise: {reading:?}"
        );
    }
}

#[test]
fn the_cost_charged_is_the_fresh_launch_figure_and_comes_from_the_snapshot() {
    // These rows are fresh launches, and 0019 measured that cohort at 850 --
    // higher than any `by_notional` band, and it declined to lower the constant
    // because rounding a cost down launders a trade past the gate. If this ever
    // reads a number written in the code, the report will say one thing and
    // charge another.
    let rates = rates();
    let table = table_of(|index, _| (pseudo(index, 99) - 0.5) * 4_000.0);
    let report = edge::run(&table, &rates, &Options::default()).expect("runs");

    assert!((report.round_trip_bps - rates.round_trip_kernel).abs() < f64::EPSILON);
    assert!(
        report.round_trip_bps > rates.round_trip_bar,
        "the fresh-launch cohort costs more than the band that circulates as the bar"
    );
    assert!((report.band_bar_bps - rates.round_trip_bar).abs() < f64::EPSILON);
    assert_eq!(report.rates_measured_on, rates.measured_on);

    // A band is still selectable, for sensitivity.
    let cheaper = edge::run(
        &table,
        &rates,
        &Options {
            cost: edge::Cost::Band(edge::A_NOTIONAL_BAND.to_owned()),
            ..Options::default()
        },
    )
    .expect("runs");
    assert!(cheaper.round_trip_bps < report.round_trip_bps);
}

#[test]
fn a_band_the_snapshot_does_not_name_is_refused_rather_than_substituted() {
    // Charging a neighbouring band because the named one was absent would make
    // the report say one cost and charge another, silently.
    let rates = rates();
    let table = table_of(|index, _| (pseudo(index, 99) - 0.5) * 4_000.0);

    let refused = edge::run(
        &table,
        &rates,
        &Options {
            cost: edge::Cost::Band("$1,000,000+".to_owned()),
            ..Options::default()
        },
    );
    assert!(matches!(refused, Err(edge::EdgeError::NoCostBand { .. })));
}

#[test]
fn too_few_rows_is_refused_rather_than_reported() {
    // Five folds over forty rows is arithmetic on noise wearing a table's
    // clothes.
    let rates = rates();
    let mut table = table_of(|index, _| pseudo(index, 1) * 100.0);
    table.rows.truncate(40);

    assert!(matches!(
        edge::run(&table, &rates, &Options::default()),
        Err(edge::EdgeError::TooFewLaunches { launches: 40 })
    ));
}

#[test]
fn the_population_floor_is_the_folds_times_the_row_floor_and_not_some_other_arithmetic() {
    // The threshold walked from both sides. One row short is refused; exactly
    // the floor is not refused *for this reason*.
    //
    // Re-apply by replacing the `*` with `+` or `/`: the population guard stops
    // firing, and the run falls through to the labelled guard -- which reports
    // the same count under a different name. That masking is why the two
    // refusals are named apart, and it is what made the mutation invisible
    // while they shared one.
    let rates = rates();
    let floor = edge::FOLDS * edge::MIN_ROWS;

    let mut short = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);
    short.rows.truncate(floor - 1);
    assert!(
        matches!(
            edge::run(&short, &rates, &Options::default()),
            Err(edge::EdgeError::TooFewLaunches { launches }) if launches == floor - 1
        ),
        "one launch short of the floor is refused as a population"
    );

    let mut exact = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);
    exact.rows.truncate(floor);
    assert!(
        !matches!(
            edge::run(&exact, &rates, &Options::default()),
            Err(edge::EdgeError::TooFewLaunches { .. })
        ),
        "exactly the floor is enough launches"
    );
}

#[test]
fn a_population_that_is_labelled_too_thinly_is_a_different_refusal() {
    // The two floors are the same number and different questions, and the
    // report has to say which one it hit: "there is nothing to split" and
    // "there is nothing to read" send an operator to different places.
    let rates = rates();
    let mut table = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);
    table.rows.truncate(edge::FOLDS * edge::MIN_ROWS + 10);
    // Plenty of launches, almost no labels.
    for row in table.rows.iter_mut().skip(5) {
        row.gross_6h_bps = None;
        row.gross_24h_bps = None;
    }

    assert!(
        matches!(
            edge::run(&table, &rates, &Options::default()),
            Err(edge::EdgeError::TooFewRows { rows: 5 })
        ),
        "enough launches to split, too few labels to read"
    );
}

#[test]
fn removing_labels_cannot_move_a_fold_boundary() {
    // The defect, stated as a property. Fold boundaries came from the
    // **labelled** rows, so a horizon whose exit price is missing for a run of
    // launches moved every later boundary -- and the same history at two
    // horizons was split two different ways. Boundaries are a property of the
    // population, and the population is every eligible launch.
    //
    // Re-apply by passing the labelled points to `split` again: the second
    // report's windows shift and this fails on the first fold.
    let full = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);

    // The same table with a quarter of its labels gone. Nothing else changes:
    // same launches, same slots, same features, same order.
    let mut sparse = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);
    for (index, row) in sparse.rows.iter_mut().enumerate() {
        if index % 4 == 0 {
            row.gross_6h_bps = None;
            row.gross_24h_bps = None;
        }
    }

    let options = Options::default();
    let a = edge::run(&full, &rates(), &options).expect("ran over the full table");
    let b = edge::run(&sparse, &rates(), &options).expect("ran over the sparse table");

    let bounds = |r: &edge::Report| -> Vec<(u64, u64)> {
        r.folds.iter().map(|f| (f.from.get(), f.to.get())).collect()
    };
    assert_eq!(
        bounds(&a),
        bounds(&b),
        "the population is identical, so the windows must be"
    );

    // And the counts say what changed, rather than the boundaries saying it.
    assert_eq!(
        a.eligible_rows, b.eligible_rows,
        "the same launches are eligible either way"
    );
    assert!(
        b.labelled_rows < a.labelled_rows,
        "a quarter of the labels are gone: {} vs {}",
        b.labelled_rows,
        a.labelled_rows
    );
    for (index, fold) in b.folds.iter().enumerate() {
        assert!(
            fold.labelled < fold.population,
            "fold {index} should show missing labels as a gap between its own \
             counts, not as a moved edge: {} labelled of {}",
            fold.labelled,
            fold.population
        );
    }
}

#[test]
fn the_cohort_counts_are_the_denominators_and_they_nest() {
    // `rows` is what a reading is taken over, `labelled` is what carried a
    // label at all, and `population` is every eligible launch in the window.
    // The two differences are different facts -- missing evidence, and the
    // purge and embargo -- and a report that collapsed them would hide the
    // first behind the second.
    let table = table_of(|_, planted| planted[0] * 40.0 - 2_000.0);
    let report = edge::run(&table, &rates(), &Options::default()).expect("ran");

    assert_eq!(
        report.eligible_rows,
        table.rows.len(),
        "every row in the table is an eligible launch"
    );
    assert_eq!(
        report.folds.iter().map(|f| f.population).sum::<usize>(),
        report.eligible_rows,
        "the windows partition the population, so their counts sum to it"
    );
    for (index, fold) in report.folds.iter().enumerate() {
        assert!(
            fold.rows <= fold.labelled,
            "fold {index}: scored rows cannot exceed labelled ones"
        );
        assert!(
            fold.labelled <= fold.population,
            "fold {index}: labelled rows cannot exceed the population"
        );
    }
}
