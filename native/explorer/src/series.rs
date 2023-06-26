use crate::{
    atoms,
    datatypes::{ExDate, ExDateTime, ExTime},
    encoding, ExDataFrame, ExSeries, ExplorerError,
};

use encoding::encode_datetime;
use polars::export::arrow::array::Utf8Array;
use polars::prelude::*;
use rustler::{Binary, Encoder, Env, ListIterator, Term, TermType};
use std::{result::Result, slice};

pub mod log;

#[rustler::nif]
pub fn s_as_str(data: ExSeries) -> Result<String, ExplorerError> {
    Ok(format!("{:?}", data.resource.0))
}

macro_rules! from_list {
    ($name:ident, $type:ty) => {
        #[rustler::nif(schedule = "DirtyCpu")]
        pub fn $name(name: &str, val: Vec<Option<$type>>) -> ExSeries {
            ExSeries::new(Series::new(name, val.as_slice()))
        }
    };
}

from_list!(s_from_list_i64, i64);
from_list!(s_from_list_u32, u32);
from_list!(s_from_list_bool, bool);
from_list!(s_from_list_str, String);

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_f64(name: &str, val: Term) -> ExSeries {
    let nan = atoms::nan();
    let infinity = atoms::infinity();
    let neg_infinity = atoms::neg_infinity();

    ExSeries::new(Series::new(
        name,
        val.decode::<ListIterator>()
            .unwrap()
            .map(|item| match item.get_type() {
                TermType::Number => Some(item.decode::<f64>().unwrap()),
                TermType::Atom => {
                    if nan.eq(&item) {
                        Some(f64::NAN)
                    } else if infinity.eq(&item) {
                        Some(f64::INFINITY)
                    } else if neg_infinity.eq(&item) {
                        Some(f64::NEG_INFINITY)
                    } else {
                        None
                    }
                }
                term_type => panic!("from_list/2 not implemented for {term_type:?}"),
            })
            .collect::<Vec<Option<f64>>>(),
    ))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_date(name: &str, val: Vec<Option<ExDate>>) -> ExSeries {
    ExSeries::new(
        Series::new(
            name,
            val.iter()
                .map(|d| d.map(|d| d.into()))
                .collect::<Vec<Option<i32>>>(),
        )
        .cast(&DataType::Date)
        .unwrap(),
    )
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_datetime(name: &str, val: Vec<Option<ExDateTime>>) -> ExSeries {
    ExSeries::new(
        Series::new(
            name,
            val.iter()
                .map(|dt| dt.map(|dt| dt.into()))
                .collect::<Vec<Option<i64>>>(),
        )
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
        .unwrap(),
    )
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_time(name: &str, val: Vec<Option<ExTime>>) -> ExSeries {
    ExSeries::new(
        Series::new(
            name,
            val.iter()
                .map(|dt| dt.map(|dt| dt.into()))
                .collect::<Vec<Option<i64>>>(),
        )
        .cast(&DataType::Time)
        .unwrap(),
    )
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_binary(name: &str, val: Vec<Option<Binary>>) -> ExSeries {
    ExSeries::new(Series::new(
        name,
        val.iter()
            .map(|bin| bin.map(|bin| bin.as_slice()))
            .collect::<Vec<Option<&[u8]>>>(),
    ))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_from_list_categories(name: &str, val: Vec<Option<String>>) -> ExSeries {
    ExSeries::new(
        Series::new(name, val.as_slice())
            .cast(&DataType::Categorical(None))
            .unwrap(),
    )
}

macro_rules! from_binary {
    ($name:ident, $type:ty, $bytes:expr) => {
        #[rustler::nif(schedule = "DirtyCpu")]
        pub fn $name(name: &str, val: Binary) -> ExSeries {
            let slice = val.as_slice();
            let transmuted = unsafe {
                slice::from_raw_parts(slice.as_ptr() as *const $type, slice.len() / $bytes)
            };
            ExSeries::new(Series::new(name, transmuted))
        }
    };
}

from_binary!(s_from_binary_f64, f64, 8);
from_binary!(s_from_binary_i32, i32, 4);
from_binary!(s_from_binary_i64, i64, 8);
from_binary!(s_from_binary_u8, u8, 1);

#[rustler::nif]
pub fn s_name(data: ExSeries) -> Result<String, ExplorerError> {
    Ok(data.name().to_string())
}

#[rustler::nif]
pub fn s_rename(data: ExSeries, name: &str) -> Result<ExSeries, ExplorerError> {
    let mut s = data.clone_inner();
    s.rename(name);
    Ok(ExSeries::new(s))
}

#[rustler::nif]
pub fn s_dtype(data: ExSeries) -> Result<String, ExplorerError> {
    let dt = data.dtype().to_string();
    Ok(dt)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_slice(series: ExSeries, offset: i64, length: usize) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.slice(offset, length)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_format(series_vec: Vec<ExSeries>) -> Result<ExSeries, ExplorerError> {
    let mut iter = series_vec.iter();
    let mut series = iter.next().unwrap().clone_inner().utf8()?.clone();

    for s in iter {
        series = series.concat(s.utf8()?);
    }

    Ok(ExSeries::new(series.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_concat(series_vec: Vec<ExSeries>) -> Result<ExSeries, ExplorerError> {
    let mut iter = series_vec.iter();
    let mut series = iter.next().unwrap().clone_inner();

    for s in iter {
        series.append(s)?;
    }

    Ok(ExSeries::new(series))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_mask(series: ExSeries, filter: ExSeries) -> Result<ExSeries, ExplorerError> {
    if let Ok(ca) = filter.bool() {
        let series = series.filter(ca)?;
        Ok(ExSeries::new(series))
    } else {
        Err(ExplorerError::Other("Expected a boolean mask".into()))
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_add(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = other.clone_inner();
    Ok(ExSeries::new(s + s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_subtract(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = other.clone_inner();
    Ok(ExSeries::new(s - s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_multiply(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = other.clone_inner();
    Ok(ExSeries::new(s * s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_divide(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner().cast(&DataType::Float64)?;
    let s1 = other.clone_inner().cast(&DataType::Float64)?;
    Ok(ExSeries::new(s / s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_quotient(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(checked_div(data, other)?))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_remainder(data: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = other.clone_inner();
    let div = checked_div(data, other)?;
    let mult = s1 * div;
    let result = s - mult;

    Ok(ExSeries::new(result))
}

// There is a bug in Polars where broadcast is not applied to checked_div
// and instead it discards values.
fn checked_div(data: ExSeries, other: ExSeries) -> Result<Series, ExplorerError> {
    match data.len() {
        1 => {
            let num = data.i64()?.get(0).unwrap();
            Ok(Series::new(
                data.name(),
                other
                    .i64()?
                    .apply_on_opt(|v| v.and_then(|v| num.checked_div(v))),
            ))
        }
        _ => match other.len() {
            1 => Ok(data.checked_div_num(other.i64()?.get(0).unwrap())?),
            _ => Ok(data.checked_div(&other)?),
        },
    }
}

#[rustler::nif]
pub fn s_head(series: ExSeries, length: Option<usize>) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.head(length)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_tail(series: ExSeries, length: Option<usize>) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.tail(length)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_shift(series: ExSeries, offset: i64) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.shift(offset)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_sort(
    series: ExSeries,
    descending: bool,
    nulls_last: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = SortOptions {
        descending,
        nulls_last,
        multithreaded: false,
    };
    Ok(ExSeries::new(series.sort_with(opts)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_argsort(
    series: ExSeries,
    descending: bool,
    nulls_last: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = SortOptions {
        descending,
        nulls_last,
        multithreaded: false,
    };
    let indices = series.arg_sort(opts).cast(&DataType::Int64)?;
    Ok(ExSeries::new(indices))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_distinct(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    let unique = series.take(&series.arg_unique()?)?;
    Ok(ExSeries::new(unique))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_unordered_distinct(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    let unique = series.unique()?;
    Ok(ExSeries::new(unique))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_frequencies(series: ExSeries) -> Result<ExDataFrame, ExplorerError> {
    let mut df = series.value_counts(true, true)?;
    let df = df
        .try_apply("counts", |s| s.cast(&DataType::Int64))?
        .clone();
    Ok(ExDataFrame::new(df))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_slice_by_indices(series: ExSeries, indices: Vec<u32>) -> Result<ExSeries, ExplorerError> {
    let idx = UInt32Chunked::from_vec("idx", indices);
    let s1 = series.take(&idx)?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_slice_by_series(series: ExSeries, indices: ExSeries) -> Result<ExSeries, ExplorerError> {
    match indices.strict_cast(&DataType::UInt32) {
        Ok(casted) => {
            let idx = casted.u32()?;
            match series.take(idx) {
                Ok(s1) => Ok(ExSeries::new(s1)),
                Err(_) => Err(ExplorerError::Other(
                    "slice/2 cannot select from indices that are out-of-bounds".into(),
                )),
            }
        }
        Err(_) => Err(ExplorerError::Other(
            "slice/2 expects a series of positive integers".into(),
        )),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_is_null(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.is_null().into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_is_not_null(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.is_not_null().into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_is_finite(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.is_finite()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_is_infinite(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.is_infinite()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_is_nan(series: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.is_nan()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_at_every(series: ExSeries, n: usize) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.take_every(n)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_series_equal(
    series: ExSeries,
    other: ExSeries,
    null_equal: bool,
) -> Result<bool, ExplorerError> {
    let result = if null_equal {
        series.series_equal_missing(&other)
    } else {
        series.series_equal(&other)
    };

    Ok(result)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_equal(lhs: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(
        lhs.clone_inner().equal(&rhs.clone_inner())?.into_series(),
    ))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_not_equal(data: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = rhs.clone_inner();
    Ok(ExSeries::new(s.not_equal(&s1)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_greater(data: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = rhs.clone_inner();
    Ok(ExSeries::new(s.gt(&s1)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_greater_equal(data: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = rhs.clone_inner();
    Ok(ExSeries::new(s.gt_eq(&s1)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_less(data: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = rhs.clone_inner();
    Ok(ExSeries::new(s.lt(&s1)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_less_equal(data: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = data.clone_inner();
    let s1 = rhs.clone_inner();
    Ok(ExSeries::new(s.lt_eq(&s1)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_in(s: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s = match s.dtype() {
        DataType::Boolean => s.bool()?.is_in(&rhs)?,
        DataType::Int64 => s.i64()?.is_in(&rhs)?,
        DataType::Float64 => s.f64()?.is_in(&rhs)?,
        DataType::Utf8 => s.utf8()?.is_in(&rhs)?,
        DataType::Binary => s.binary()?.is_in(&rhs)?,
        DataType::Date => s.date()?.is_in(&rhs)?,
        DataType::Time => s.time()?.is_in(&rhs)?,
        DataType::Datetime(_, _) => s.datetime()?.is_in(&rhs)?,
        dt => panic!("is_in/2 not implemented for {dt:?}"),
    };

    Ok(ExSeries::new(s.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_and(lhs: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let and = lhs.bool()? & rhs.bool()?;
    Ok(ExSeries::new(and.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_or(lhs: ExSeries, rhs: ExSeries) -> Result<ExSeries, ExplorerError> {
    let or = lhs.bool()? | rhs.bool()?;
    Ok(ExSeries::new(or.into_series()))
}

#[rustler::nif]
pub fn s_size(series: ExSeries) -> Result<usize, ExplorerError> {
    Ok(series.len())
}

#[rustler::nif]
pub fn s_nil_count(series: ExSeries) -> Result<usize, ExplorerError> {
    Ok(series.null_count())
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_strategy(
    series: ExSeries,
    strategy: &str,
) -> Result<ExSeries, ExplorerError> {
    let strat = match strategy {
        "backward" => FillNullStrategy::Backward(None),
        "forward" => FillNullStrategy::Forward(None),
        "min" => FillNullStrategy::Min,
        "max" => FillNullStrategy::Max,
        "mean" => FillNullStrategy::Mean,
        s => return Err(ExplorerError::Other(format!("Strategy {s} not supported"))),
    };

    Ok(ExSeries::new(series.fill_null(strat)?))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_atom(series: ExSeries, atom: &str) -> Result<ExSeries, ExplorerError> {
    let value = cast_str_to_f64(atom);
    let s = series.f64()?.fill_null_with_values(value)?.into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_int(series: ExSeries, integer: i64) -> Result<ExSeries, ExplorerError> {
    let s = series.i64()?.fill_null_with_values(integer)?.into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_float(series: ExSeries, float: f64) -> Result<ExSeries, ExplorerError> {
    let s = series.f64()?.fill_null_with_values(float)?.into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_bin(
    series: ExSeries,
    binary: Binary,
) -> Result<ExSeries, ExplorerError> {
    let s = match series.dtype() {
        DataType::Utf8 => {
            if let Ok(_string) = std::str::from_utf8(&binary) {
                // This casting is necessary just because it's not possible to fill UTF8 series.
                unsafe {
                    series
                        .cast_unchecked(&DataType::Binary)?
                        .binary()?
                        .fill_null_with_values(&binary)?
                        .cast_unchecked(&DataType::Utf8)?
                }
            } else {
                return Err(ExplorerError::Other("cannot cast to string".into()));
            }
        }
        DataType::Binary => series
            .binary()?
            .fill_null_with_values(&binary)?
            .into_series(),
        dt => panic!("fill_missing/2 not implemented for {dt:?}"),
    };
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_date(series: ExSeries, date: ExDate) -> Result<ExSeries, ExplorerError> {
    let s = series
        .date()?
        .fill_null_with_values(date.into())?
        .cast(&DataType::Date)?
        .into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_datetime(
    series: ExSeries,
    datetime: ExDateTime,
) -> Result<ExSeries, ExplorerError> {
    let s = series
        .datetime()?
        .fill_null_with_values(datetime.into())?
        .cast(series.dtype())?
        .into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_fill_missing_with_boolean(
    series: ExSeries,
    boolean: bool,
) -> Result<ExSeries, ExplorerError> {
    let s = series.bool()?.fill_null_with_values(boolean)?.into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_window_sum(
    series: ExSeries,
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = rolling_opts(window_size, weights, min_periods, center);
    let s1 = series.rolling_sum(opts.into())?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_window_mean(
    series: ExSeries,
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = rolling_opts(window_size, weights, min_periods, center);
    let s1 = series.rolling_mean(opts.into())?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_window_max(
    series: ExSeries,
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = rolling_opts(window_size, weights, min_periods, center);
    let s1 = series.rolling_max(opts.into())?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_window_min(
    series: ExSeries,
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = rolling_opts(window_size, weights, min_periods, center);
    let s1 = series.rolling_min(opts.into())?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_window_standard_deviation(
    series: ExSeries,
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = rolling_opts(window_size, weights, min_periods, center);
    let s1 = series.rolling_std(opts.into())?;
    Ok(ExSeries::new(s1))
}

// Used for rolling functions - also see "expressions" module
pub fn rolling_opts(
    window_size: usize,
    weights: Option<Vec<f64>>,
    min_periods: Option<usize>,
    center: bool,
) -> RollingOptions {
    let min_periods = if let Some(mp) = min_periods {
        mp
    } else {
        window_size
    };
    let window_size_duration = Duration::new(window_size as i64);

    RollingOptions {
        window_size: window_size_duration,
        weights,
        min_periods,
        center,
        ..Default::default()
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_ewm_mean(
    series: ExSeries,
    alpha: f64,
    adjust: bool,
    min_periods: usize,
    ignore_nulls: bool,
) -> Result<ExSeries, ExplorerError> {
    let opts = ewm_opts(alpha, adjust, min_periods, ignore_nulls);
    let s1 = series.ewm_mean(opts)?;
    Ok(ExSeries::new(s1))
}

pub fn ewm_opts(alpha: f64, adjust: bool, min_periods: usize, ignore_nulls: bool) -> EWMOptions {
    EWMOptions {
        alpha,
        adjust,
        min_periods,
        ignore_nulls,
        ..Default::default()
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_to_list(env: Env, data: ExSeries) -> Result<Term, ExplorerError> {
    encoding::list_from_series(data, env)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_to_iovec(env: Env, series: ExSeries) -> Result<Term, ExplorerError> {
    if series.null_count() != 0 {
        Err(ExplorerError::Other(
            "cannot invoke to_iovec on series with nils".into(),
        ))
    } else {
        encoding::iovec_from_series(series, env)
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_sum(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Boolean => Ok(s.sum::<i64>().encode(env)),
        DataType::Int64 => Ok(s.sum::<i64>().encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.sum::<f64>(), env)),
        dt => panic!("sum/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_min(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(s.min::<i64>().encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.min::<f64>(), env)),
        DataType::Date => Ok(s.min::<i32>().map(ExDate::from).encode(env)),
        DataType::Time => Ok(s.min::<i64>().map(ExTime::from).encode(env)),
        DataType::Datetime(unit, None) => Ok(s
            .min::<i64>()
            .map(|v| encode_datetime(v, *unit, env).unwrap())
            .encode(env)),
        dt => panic!("min/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_max(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(s.max::<i64>().encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.max::<f64>(), env)),
        DataType::Date => Ok(s.max::<i32>().map(ExDate::from).encode(env)),
        DataType::Time => Ok(s.max::<i64>().map(ExTime::from).encode(env)),
        DataType::Datetime(unit, None) => Ok(s
            .max::<i64>()
            .map(|v| encode_datetime(v, *unit, env).unwrap())
            .encode(env)),
        dt => panic!("max/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_mean(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Boolean => Ok(s.mean().encode(env)),
        DataType::Int64 => Ok(s.mean().encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.mean(), env)),
        dt => panic!("mean/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_median(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(s.median().encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.median(), env)),
        dt => panic!("median/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_product(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(ExSeries::new(s.product())),
        DataType::Float64 => Ok(ExSeries::new(s.product())),
        dt => panic!("product/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_variance(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(s.i64()?.var(1).encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.f64()?.var(1), env)),
        dt => panic!("var/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_standard_deviation(env: Env, s: ExSeries) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => Ok(s.i64()?.std(1).encode(env)),
        DataType::Float64 => Ok(term_from_optional_float(s.f64()?.std(1), env)),
        dt => panic!("std/1 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_skew(env: Env, s: ExSeries, bias: bool) -> Result<Term, ExplorerError> {
    match s.dtype() {
        DataType::Float64 => Ok(s.skew(bias)?.encode(env)),
        DataType::Int64 => Ok(s.skew(bias)?.encode(env)),
        // DataType::Float64 => Ok(term_from_optional_float(s.skew(bias), env)),
        dt => panic!("skew/2 not implemented for {dt:?}"),
    }
}

fn term_from_optional_float(option: Option<f64>, env: Env<'_>) -> Term<'_> {
    match option {
        Some(float) => encoding::term_from_float(float, env),
        None => rustler::types::atom::nil().to_term(env),
    }
}

#[rustler::nif]
pub fn s_at(env: Env, series: ExSeries, idx: usize) -> Result<Term, ExplorerError> {
    encoding::resource_term_from_value(&series.resource, series.get(idx)?, env)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cumulative_sum(series: ExSeries, reverse: bool) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.cumsum(reverse)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cumulative_max(series: ExSeries, reverse: bool) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.cummax(reverse)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cumulative_min(series: ExSeries, reverse: bool) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.cummin(reverse)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cumulative_product(series: ExSeries, reverse: bool) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(series.cumprod(reverse)))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_quantile<'a>(
    env: Env<'a>,
    s: ExSeries,
    quantile: f64,
    strategy: &str,
) -> Result<Term<'a>, ExplorerError> {
    let dtype = s.dtype();
    let strategy = parse_quantile_interpol_options(strategy);
    match dtype {
        DataType::Date => match s.date()?.quantile(quantile, strategy)? {
            None => Ok(None::<ExDate>.encode(env)),
            Some(days) => Ok(ExDate::from(days as i32).encode(env)),
        },
        DataType::Time => match s.time()?.quantile(quantile, strategy)? {
            None => Ok(None::<ExTime>.encode(env)),
            Some(microseconds) => Ok(ExTime::from(microseconds as i64).encode(env)),
        },
        DataType::Datetime(unit, None) => match s.datetime()?.quantile(quantile, strategy)? {
            None => Ok(None::<ExDateTime>.encode(env)),
            Some(time) => Ok(encode_datetime(time as i64, *unit, env)
                .unwrap()
                .encode(env)),
        },
        _ => encoding::term_from_value(
            s.quantile_as_series(quantile, strategy)?
                .cast(dtype)?
                .get(0)?,
            env,
        ),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_peak_max(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.peak_max().into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_peak_min(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.peak_min().into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_reverse(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.reverse()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_n_distinct(s: ExSeries) -> Result<usize, ExplorerError> {
    Ok(s.n_unique()?)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_pow(s: ExSeries, other: ExSeries) -> Result<ExSeries, ExplorerError> {
    match s.dtype() {
        DataType::Int64 => {
            let iter1 = s.i64()?.into_iter();
            match other.strict_cast(&DataType::UInt32) {
                Ok(casted) => {
                    let iter2 = casted.u32()?.into_iter();

                    let s = iter1
                        .zip(iter2)
                        .map(|(v1, v2)| v1.and_then(|left| v2.map(|right| left.pow(right))))
                        .collect();

                    Ok(ExSeries::new(s))
                }
                Err(_) => Err(ExplorerError::Other(
                    "negative exponent with an integer base".into(),
                )),
            }
        }
        DataType::Float64 => {
            let iter1 = s.f64()?.into_iter();
            let iter2 = other.f64()?.into_iter();
            let s = iter1
                .zip(iter2)
                .map(|(v1, v2)| v1.and_then(|left| v2.map(|right| left.powf(right))))
                .collect();

            Ok(ExSeries::new(s))
        }
        dt => panic!("pow/2 not implemented for {dt:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_pow_f_rhs(s: ExSeries, exponent: Term) -> Result<ExSeries, ExplorerError> {
    let nan = atoms::nan();
    let infinity = atoms::infinity();
    let neg_infinity = atoms::neg_infinity();

    let float = match exponent.get_type() {
        TermType::Number => exponent.decode::<f64>().unwrap(),
        TermType::Atom => {
            if nan.eq(&exponent) {
                f64::NAN
            } else if infinity.eq(&exponent) {
                f64::INFINITY
            } else if neg_infinity.eq(&exponent) {
                f64::NEG_INFINITY
            } else {
                panic!("pow/2 invalid float")
            }
        }
        term_type => panic!("pow/2 not implemented for {term_type:?}"),
    };

    let s = s
        .cast(&DataType::Float64)?
        .f64()?
        .apply(|v| v.powf(float))
        .into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_pow_f_lhs(s: ExSeries, exponent: Term) -> Result<ExSeries, ExplorerError> {
    let nan = atoms::nan();
    let infinity = atoms::infinity();
    let neg_infinity = atoms::neg_infinity();

    let float = match exponent.get_type() {
        TermType::Number => exponent.decode::<f64>().unwrap(),
        TermType::Atom => {
            if nan.eq(&exponent) {
                f64::NAN
            } else if infinity.eq(&exponent) {
                f64::INFINITY
            } else if neg_infinity.eq(&exponent) {
                f64::NEG_INFINITY
            } else {
                panic!("pow/2 invalid float")
            }
        }
        term_type => panic!("pow/2 not implemented for {term_type:?}"),
    };

    let s = s
        .cast(&DataType::Float64)?
        .f64()?
        .apply(|v| float.powf(v))
        .into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_pow_i_rhs(s: ExSeries, exponent: u32) -> Result<ExSeries, ExplorerError> {
    let s = s.i64()?.apply(|v| v.pow(exponent)).into_series();
    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_pow_i_lhs(s: ExSeries, base: i64) -> Result<ExSeries, ExplorerError> {
    let s = s
        .i64()?
        .try_apply(|v| match u32::try_from(v) {
            Ok(v) => Ok(base.pow(v)),
            Err(_) => Err(PolarsError::ComputeError(
                "negative exponent with an integer base".into(),
            )),
        })?
        .into_series();

    Ok(ExSeries::new(s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cast(s: ExSeries, to_type: &str) -> Result<ExSeries, ExplorerError> {
    let dtype = cast_str_to_dtype(to_type)?;
    Ok(ExSeries::new(s.cast(&dtype)?))
}

pub fn cast_str_to_dtype(str_type: &str) -> Result<DataType, ExplorerError> {
    match str_type {
        "float" => Ok(DataType::Float64),
        "integer" => Ok(DataType::Int64),
        "date" => Ok(DataType::Date),
        "time" => Ok(DataType::Time),
        "datetime" => Ok(DataType::Datetime(TimeUnit::Microseconds, None)),
        "boolean" => Ok(DataType::Boolean),
        "string" => Ok(DataType::Utf8),
        "binary" => Ok(DataType::Binary),
        "category" => Ok(DataType::Categorical(None)),
        _ => Err(ExplorerError::Other(format!(
            "Cannot cast to dtype: {str_type}"
        ))),
    }
}

pub fn cast_str_to_f64(atom: &str) -> f64 {
    match atom {
        "nan" => f64::NAN,
        "infinity" => f64::INFINITY,
        "neg_infinity" => f64::NEG_INFINITY,
        _ => panic!("unknown literal {atom:?}"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_categories(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    match s.dtype() {
        DataType::Categorical(Some(mapping)) => {
            let size = mapping.len() as u32;
            let categories: Vec<&str> = (0..size).map(|id| mapping.get(id)).collect();
            let series = Series::new("categories", &categories);
            Ok(ExSeries::new(series))
        }
        _ => panic!("Cannot get categories from non categorical series"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_categorise(s: ExSeries, cat: ExSeries) -> Result<ExSeries, ExplorerError> {
    let chunks = s.cast(&DataType::UInt32)?.u32()?.clone();

    match cat.dtype() {
        DataType::Categorical(Some(mapping)) => {
            let categorical_chunks = unsafe {
                CategoricalChunked::from_cats_and_rev_map_unchecked(chunks, mapping.clone())
            };
            Ok(ExSeries::new(categorical_chunks.into_series()))
        }
        DataType::Utf8 => {
            if cat.len() != cat.unique()?.len() {
                return Err(ExplorerError::Other(
                    "categories as strings cannot have duplicated values".into(),
                ));
            };

            let utf8s = cat.utf8()?;

            if utf8s.has_validity() {
                Err(ExplorerError::Other(
                    "categories as strings cannot have nil values".into(),
                ))
            } else {
                let values: Vec<Option<&str>> = utf8s.into();
                let array = Utf8Array::<i64>::from(values);
                let mapping = RevMapping::Local(array);

                let categorical_chunks = unsafe {
                    CategoricalChunked::from_cats_and_rev_map_unchecked(chunks, Arc::new(mapping))
                };

                Ok(ExSeries::new(categorical_chunks.into_series()))
            }
        }
        _ => panic!("Cannot get categories from non categorical or string series"),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_sample_n(
    series: ExSeries,
    n: usize,
    replace: bool,
    shuffle: bool,
    seed: Option<u64>,
) -> Result<ExSeries, ExplorerError> {
    let new_s = series.sample_n(n, replace, shuffle, seed)?;

    Ok(ExSeries::new(new_s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_sample_frac(
    series: ExSeries,
    frac: f64,
    replace: bool,
    shuffle: bool,
    seed: Option<u64>,
) -> Result<ExSeries, ExplorerError> {
    let new_s = series.sample_frac(frac, replace, shuffle, seed)?;

    Ok(ExSeries::new(new_s))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_rank(
    series: ExSeries,
    method: &str,
    descending: bool,
    seed: Option<u64>,
) -> Result<ExSeries, ExplorerError> {
    let rank_method = parse_rank_method_options(method, descending);

    match rank_method.method {
        RankMethod::Average => {
            let new_s = series
                .rank(rank_method, seed)
                .cast(&DataType::Float64)?
                .into_series();

            Ok(ExSeries::new(new_s))
        }
        _ => {
            let new_s = series
                .rank(rank_method, seed)
                .cast(&DataType::Int64)?
                .into_series();

            Ok(ExSeries::new(new_s))
        }
    }
}

pub fn parse_rank_method_options(strategy: &str, descending: bool) -> RankOptions {
    match strategy {
        "ordinal" => RankOptions {
            method: RankMethod::Ordinal,
            descending,
        },
        "random" => RankOptions {
            method: RankMethod::Random,
            descending,
        },
        "average" => RankOptions {
            method: RankMethod::Average,
            descending,
        },
        "min" => RankOptions {
            method: RankMethod::Min,
            descending,
        },
        "max" => RankOptions {
            method: RankMethod::Max,
            descending,
        },
        "dense" => RankOptions {
            method: RankMethod::Dense,
            descending,
        },
        _ => RankOptions {
            method: RankMethod::Average,
            descending,
        },
    }
}

pub fn parse_quantile_interpol_options(strategy: &str) -> QuantileInterpolOptions {
    match strategy {
        "nearest" => QuantileInterpolOptions::Nearest,
        "lower" => QuantileInterpolOptions::Lower,
        "higher" => QuantileInterpolOptions::Higher,
        "midpoint" => QuantileInterpolOptions::Midpoint,
        "linear" => QuantileInterpolOptions::Linear,
        _ => QuantileInterpolOptions::Nearest,
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_coalesce(s1: ExSeries, s2: ExSeries) -> Result<ExSeries, ExplorerError> {
    let coalesced = s1.zip_with(&s1.is_not_null(), &s2)?;
    Ok(ExSeries::new(coalesced))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_select(
    pred: ExSeries,
    on_true: ExSeries,
    on_false: ExSeries,
) -> Result<ExSeries, ExplorerError> {
    match pred.len() {
        1 => match pred.bool().unwrap().get(0).unwrap() {
            true => Ok(on_true),
            false => Ok(on_false),
        },
        _ => {
            let selected = on_true.zip_with(pred.bool().unwrap(), &on_false)?;
            Ok(ExSeries::new(selected))
        }
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_not(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s2 = s1
        .bool()?
        .into_iter()
        .map(|opt_v| opt_v.map(|v| !v))
        .collect();

    Ok(ExSeries::new(s2))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_contains(s1: ExSeries, pattern: &str) -> Result<ExSeries, ExplorerError> {
    // TODO: maybe have "strict" as an option.
    Ok(ExSeries::new(s1.utf8()?.contains(pattern, true)?.into()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_upcase(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s1.utf8()?.to_uppercase().into()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_downcase(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s1.utf8()?.to_lowercase().into()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_trim(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    // There are no eager strip functions.
    Ok(ExSeries::new(
        s1.utf8()?.replace(r#"^[ \s]+|[ \s]+$"#, "")?.into(),
    ))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_trim_leading(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    // There are no eager strip functions.
    Ok(ExSeries::new(s1.utf8()?.replace(r#"^[ \s]+"#, "")?.into()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_trim_trailing(s1: ExSeries) -> Result<ExSeries, ExplorerError> {
    // There are no eager strip functions.
    Ok(ExSeries::new(s1.utf8()?.replace(r#"[ \s]+$"#, "")?.into()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_round(s: ExSeries, decimals: u32) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.round(decimals)?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_floor(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.floor()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_ceil(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.ceil()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_abs(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    Ok(ExSeries::new(s.abs()?.into_series()))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_day_of_week(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.weekday()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_month(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.month()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_year(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.year()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_hour(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.hour()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_minute(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.minute()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_second(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.second()?.cast(&DataType::Int64)?;

    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_date_diff(left: ExSeries, right: ExSeries) -> Result<ExSeries, ExplorerError> {
    let left_series = left.clone_inner();
    let right_series = right.clone_inner();
    let series = (left_series - right_series)
        .duration()?
        .cast(&DataType::Int64)?;
    Ok(ExSeries::new(series / 86400000))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_strptime(s: ExSeries, format_string: &str) -> Result<ExSeries, ExplorerError> {
    let s1 = s
        .utf8()?
        .as_datetime(
            Some(format_string),
            TimeUnit::Microseconds,
            true,
            false,
            false,
            None,
        )?
        .into_series();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_strftime(s: ExSeries, format_string: &str) -> Result<ExSeries, ExplorerError> {
    let s1 = s.strftime(format_string)?;
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_sin(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.sin()).into();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_cos(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.cos()).into();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_tan(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.tan()).into();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_asin(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.asin()).into();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_acos(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.acos()).into();
    Ok(ExSeries::new(s1))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn s_atan(s: ExSeries) -> Result<ExSeries, ExplorerError> {
    let s1 = s.f64()?.apply(|o| o.atan()).into();
    Ok(ExSeries::new(s1))
}
