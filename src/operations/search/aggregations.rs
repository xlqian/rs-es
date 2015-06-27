/*
 * Copyright 2015 Ben Ashford
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Implementation of ElasticSearch [aggregations](https://www.elastic.co/guide/en/elasticsearch/reference/current/search-aggregations.html)

use std::collections::{BTreeMap, HashMap};
use std::marker::PhantomData;

use rustc_serialize::json::{Json, ToJson};

use error::EsError;
use query;
use units::{GeoBox, JsonVal};

#[derive(Debug)]
pub enum Scripts<'a> {
    Inline(&'a str, Option<&'a str>),
    Id(&'a str)
}

/// Script attributes for various attributes
#[derive(Debug)]
pub struct Script<'a> {
    script: Scripts<'a>,
    params: Option<Json>
}

impl<'a> Script<'a> {
    pub fn id(script_id: &'a str) -> Script<'a> {
        Script {
            script: Scripts::Id(script_id),
            params: None
        }
    }

    pub fn script(script: &'a str) -> Script<'a> {
        Script {
            script: Scripts::Inline(script, None),
            params: None
        }
    }

    pub fn script_and_field(script: &'a str, field: &'a str) -> Script<'a> {
        Script {
            script: Scripts::Inline(script, Some(field)),
            params: None
        }
    }

    fn add_to_object(&self, obj: &mut BTreeMap<String, Json>) {
        match self.script {
            Scripts::Inline(script, field) => {
                obj.insert("script".to_owned(), script.to_json());
                match field {
                    Some(f) => {
                        obj.insert("field".to_owned(), f.to_json());
                    },
                    None    => ()
                }
            },
            Scripts::Id(script_id) => {
                obj.insert("script_id".to_owned(), script_id.to_json());
            }
        };
        match self.params {
            Some(ref json) => {
                obj.insert("params".to_owned(), json.clone());
            },
            None           => ()
        };
    }

    pub fn with_params(mut self, params: Json) -> Self {
        self.params = Some(params);
        self
    }
}

macro_rules! metrics_agg {
    ($b:ident) => {
        impl<'a> From<$b<'a>> for Aggregation<'a> {
            fn from(from: $b<'a>) -> Aggregation<'a> {
                Aggregation::Metrics(MetricsAggregation::$b(from))
            }
        }
    }
}

/// A common pattern is for an aggregation to accept a field or a script
#[derive(Debug)]
pub enum FieldOrScript<'a> {
    Field(&'a str),
    Script(Script<'a>)
}

impl<'a> FieldOrScript<'a> {
    fn add_to_object(&self, obj: &mut BTreeMap<String, Json>) {
        match self {
            &FieldOrScript::Field(field) => {
                obj.insert("field".to_owned(), field.to_json());
            },
            &FieldOrScript::Script(ref script) => {
                script.add_to_object(obj);
            }
        }
    }
}

impl<'a> From<&'a str> for FieldOrScript<'a> {
    fn from(from: &'a str) -> FieldOrScript<'a> {
        FieldOrScript::Field(from)
    }
}

impl<'a> From<Script<'a>> for FieldOrScript<'a> {
    fn from(from: Script<'a>) -> FieldOrScript<'a> {
        FieldOrScript::Script(from)
    }
}

/// Macros to build simple `FieldOrScript` based aggregations
macro_rules! field_or_script_new {
    ($t:ident) => {
        impl<'a> $t<'a> {
            pub fn new<FOS: Into<FieldOrScript<'a>>>(fos: FOS) -> $t<'a> {
                $t(fos.into())
            }
        }
    }
}

macro_rules! field_or_script_to_json {
    ($t:ident) => {
        impl<'a> ToJson for $t<'a> {
            fn to_json(&self) -> Json {
                let mut d = BTreeMap::new();
                self.0.add_to_object(&mut d);
                Json::Object(d)
            }
        }
    }
}

/// Min aggregation
#[derive(Debug)]
pub struct Min<'a>(FieldOrScript<'a>);

field_or_script_new!(Min);
field_or_script_to_json!(Min);
metrics_agg!(Min);

/// Max aggregation
#[derive(Debug)]
pub struct Max<'a>(FieldOrScript<'a>);

field_or_script_new!(Max);
field_or_script_to_json!(Max);
metrics_agg!(Max);

/// Sum aggregation
#[derive(Debug)]
pub struct Sum<'a>(FieldOrScript<'a>);

field_or_script_new!(Sum);
field_or_script_to_json!(Sum);
metrics_agg!(Sum);

/// Avg aggregation
#[derive(Debug)]
pub struct Avg<'a>(FieldOrScript<'a>);

field_or_script_new!(Avg);
field_or_script_to_json!(Avg);
metrics_agg!(Avg);

/// Stats aggregation
#[derive(Debug)]
pub struct Stats<'a>(FieldOrScript<'a>);

field_or_script_new!(Stats);
field_or_script_to_json!(Stats);
metrics_agg!(Stats);

/// Extended stats aggregation
#[derive(Debug)]
pub struct ExtendedStats<'a>(FieldOrScript<'a>);

field_or_script_new!(ExtendedStats);
field_or_script_to_json!(ExtendedStats);
metrics_agg!(ExtendedStats);

/// Value count aggregation
#[derive(Debug)]
pub struct ValueCount<'a>(FieldOrScript<'a>);

field_or_script_new!(ValueCount);
field_or_script_to_json!(ValueCount);
metrics_agg!(ValueCount);

/// Percentiles aggregation, see: https://www.elastic.co/guide/en/elasticsearch/reference/current/search-aggregations-metrics-percentile-aggregation.html
///
/// # Examples
///
/// ```
/// use rs_es::operations::search::aggregations::Percentiles;
///
/// let p1 = Percentiles::new("field_name").with_compression(100);
/// let p2 = Percentiles::new("field_name").with_percents(vec![10.0, 20.0]);
/// ```
#[derive(Debug)]
pub struct Percentiles<'a> {
    fos:         FieldOrScript<'a>,
    percents:    Option<Vec<f64>>,
    compression: Option<u64>
}

impl<'a> Percentiles<'a> {
    pub fn new<F: Into<FieldOrScript<'a>>>(fos: F) -> Percentiles<'a> {
        Percentiles {
            fos:         fos.into(),
            percents:    None,
            compression: None
        }
    }

    add_field!(with_percents, percents, Vec<f64>);
    add_field!(with_compression, compression, u64);
}

impl<'a> ToJson for Percentiles<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        self.fos.add_to_object(&mut d);
        optional_add!(d, self.percents, "percents");
        optional_add!(d, self.compression, "compression");
        Json::Object(d)
    }
}

metrics_agg!(Percentiles);

/// Percentile Ranks aggregation
#[derive(Debug)]
pub struct PercentileRanks<'a> {
    fos:    FieldOrScript<'a>,
    values: Vec<f64>
}

impl<'a> PercentileRanks<'a> {
    pub fn new<F: Into<FieldOrScript<'a>>>(fos: F, vals: Vec<f64>) -> PercentileRanks<'a> {
        PercentileRanks {
            fos:    fos.into(),
            values: vals
        }
    }
}

impl<'a> ToJson for PercentileRanks<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        self.fos.add_to_object(&mut d);
        d.insert("values".to_owned(), self.values.to_json());
        Json::Object(d)
    }
}

metrics_agg!(PercentileRanks);

/// Cardinality aggregation
#[derive(Debug)]
pub struct Cardinality<'a> {
    fos:                 FieldOrScript<'a>,
    precision_threshold: Option<u64>,
    rehash:              Option<bool>
}

impl<'a> Cardinality<'a> {
    pub fn new<F: Into<FieldOrScript<'a>>>(fos: F) -> Cardinality<'a> {
        Cardinality {
            fos:                 fos.into(),
            precision_threshold: None,
            rehash:              None
        }
    }

    add_field!(with_precision_threshold, precision_threshold, u64);
    add_field!(with_rehash, rehash, bool);
}

impl<'a> ToJson for Cardinality<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        self.fos.add_to_object(&mut d);
        optional_add!(d, self.precision_threshold, "precision_threshold");
        optional_add!(d, self.rehash, "rehash");
        Json::Object(d)
    }
}

metrics_agg!(Cardinality);

/// Geo Bounds aggregation
#[derive(Debug)]
pub struct GeoBounds<'a> {
    field:          &'a str,
    wrap_longitude: Option<bool>
}

impl<'a> GeoBounds<'a> {
    pub fn new(field: &'a str) -> GeoBounds {
        GeoBounds {
            field: field,
            wrap_longitude: None
        }
    }

    add_field!(with_wrap_longitude, wrap_longitude, bool);
}

impl<'a> ToJson for GeoBounds<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        d.insert("field".to_owned(), self.field.to_json());
        optional_add!(d, self.wrap_longitude, "wrap_longitude");
        Json::Object(d)
    }
}

metrics_agg!(GeoBounds);

/// Scripted method aggregation
#[derive(Debug)]
pub struct ScriptedMetric<'a> {
    init_script:         Option<&'a str>,
    map_script:          &'a str,
    combine_script:      Option<&'a str>,
    reduce_script:       Option<&'a str>,
    params:              Option<Json>,
    reduce_params:       Option<Json>,
    lang:                Option<&'a str>,
    init_script_file:    Option<&'a str>,
    init_script_id:      Option<&'a str>,
    map_script_file:     Option<&'a str>,
    map_script_id:       Option<&'a str>,
    combine_script_file: Option<&'a str>,
    combine_script_id:   Option<&'a str>,
    reduce_script_file:  Option<&'a str>,
    reduce_script_id:    Option<&'a str>
}

impl<'a> ScriptedMetric<'a> {
    pub fn new(map_script: &'a str) -> ScriptedMetric<'a> {
        ScriptedMetric {
            init_script:         None,
            map_script:          map_script,
            combine_script:      None,
            reduce_script:       None,
            params:              None,
            reduce_params:       None,
            lang:                None,
            init_script_file:    None,
            init_script_id:      None,
            map_script_file:     None,
            map_script_id:       None,
            combine_script_file: None,
            combine_script_id:   None,
            reduce_script_file:  None,
            reduce_script_id:    None
        }
    }

    add_field!(with_init_script, init_script, &'a str);
    add_field!(with_combine_script, combine_script, &'a str);
    add_field!(with_reduce_script, reduce_script, &'a str);
    add_field!(with_params, params, Json);
    add_field!(with_reduce_params, reduce_params, Json);
    add_field!(with_lang, lang, &'a str);
    add_field!(with_init_script_file, init_script_file, &'a str);
    add_field!(with_init_script_id, init_script_id, &'a str);
    add_field!(with_map_script_file, map_script_file, &'a str);
    add_field!(with_map_script_id, map_script_id, &'a str);
    add_field!(with_combine_script_file, combine_script_file, &'a str);
    add_field!(with_combine_script_id, combine_script_id, &'a str);
    add_field!(with_reduce_script_file, reduce_script_file, &'a str);
    add_field!(with_reduce_script_id, reduce_script_id, &'a str);
}

impl<'a> ToJson for ScriptedMetric<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        d.insert("map_script".to_owned(), self.map_script.to_json());
        optional_add!(d, self.init_script, "init_script");
        optional_add!(d, self.combine_script, "combine_script");
        optional_add!(d, self.reduce_script, "reduce_script");
        optional_add!(d, self.params, "params");
        optional_add!(d, self.reduce_params, "reduce_params");
        optional_add!(d, self.lang, "lang");
        optional_add!(d, self.init_script_file, "init_script_file");
        optional_add!(d, self.init_script_id, "init_script_id");
        optional_add!(d, self.map_script_file, "map_script_file");
        optional_add!(d, self.map_script_id, "map_script_id");
        optional_add!(d, self.combine_script_file, "combine_script_file");
        optional_add!(d, self.combine_script_id, "combine_script_id");
        optional_add!(d, self.reduce_script_file, "reduce_script_file");
        optional_add!(d, self.reduce_script_id, "reduce_script_id");
        Json::Object(d)
    }
}

/// Individual aggregations and their options
#[derive(Debug)]
pub enum MetricsAggregation<'a> {
    Min(Min<'a>),
    Max(Max<'a>),
    Sum(Sum<'a>),
    Avg(Avg<'a>),
    Stats(Stats<'a>),
    ExtendedStats(ExtendedStats<'a>),
    ValueCount(ValueCount<'a>),
    Percentiles(Percentiles<'a>),
    PercentileRanks(PercentileRanks<'a>),
    Cardinality(Cardinality<'a>),
    GeoBounds(GeoBounds<'a>),
    ScriptedMetric(ScriptedMetric<'a>)
}

impl<'a> ToJson for MetricsAggregation<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        match self {
            &MetricsAggregation::Min(ref min_agg) => {
                d.insert("min".to_owned(), min_agg.to_json());
            },
            &MetricsAggregation::Max(ref max_agg) => {
                d.insert("max".to_owned(), max_agg.to_json());
            },
            &MetricsAggregation::Sum(ref sum_agg) => {
                d.insert("sum".to_owned(), sum_agg.to_json());
            },
            &MetricsAggregation::Avg(ref avg_agg) => {
                d.insert("avg".to_owned(), avg_agg.to_json());
            },
            &MetricsAggregation::Stats(ref stats_agg) => {
                d.insert("stats".to_owned(), stats_agg.to_json());
            },
            &MetricsAggregation::ExtendedStats(ref ext_stat_agg) => {
                d.insert("extended_stats".to_owned(), ext_stat_agg.to_json());
            },
            &MetricsAggregation::ValueCount(ref vc_agg) => {
                d.insert("value_count".to_owned(), vc_agg.to_json());
            },
            &MetricsAggregation::Percentiles(ref pc_agg) => {
                d.insert("percentiles".to_owned(), pc_agg.to_json());
            },
            &MetricsAggregation::PercentileRanks(ref pr_agg) => {
                d.insert("percentile_ranks".to_owned(), pr_agg.to_json());
            },
            &MetricsAggregation::Cardinality(ref card_agg) => {
                d.insert("cardinality".to_owned(), card_agg.to_json());
            },
            &MetricsAggregation::GeoBounds(ref gb_agg) => {
                d.insert("geo_bounds".to_owned(), gb_agg.to_json());
            },
            &MetricsAggregation::ScriptedMetric(ref sm_agg) => {
                d.insert("scripted_metric".to_owned(), sm_agg.to_json());
            }
        }
        Json::Object(d)
    }
}

// Bucket aggregations

macro_rules! bucket_agg {
    ($b:ident) => {
        impl<'a> From<($b<'a>, Aggregations<'a>)> for Aggregation<'a> {
            fn from(from: ($b<'a>, Aggregations<'a>)) -> Aggregation<'a> {
                Aggregation::Bucket(BucketAggregation::$b(from.0), Some(from.1))
            }
        }

        impl<'a> From<$b<'a>> for Aggregation<'a> {
            fn from(from: $b<'a>) -> Aggregation<'a> {
                Aggregation::Bucket(BucketAggregation::$b(from), None)
            }
        }
    }
}

/// Global aggregation, defines a single global bucket.  Can only be used as a
/// top-level aggregation.  See: https://www.elastic.co/guide/en/elasticsearch/reference/current/search-aggregations-bucket-global-aggregation.html
#[derive(Debug)]
pub struct Global<'a> {
    /// Needed for lifecycle reasons
    phantom: PhantomData<&'a str>
}

impl<'a> Global<'a> {
    pub fn new() -> Global<'a> {
        Global {
            phantom: PhantomData
        }
    }
}

impl<'a> ToJson for Global<'a> {
    fn to_json(&self) -> Json {
        Json::Object(BTreeMap::new())
    }
}

bucket_agg!(Global);

/// Filter aggregation
#[derive(Debug)]
pub struct Filter<'a> {
    filter: &'a query::Filter
}

impl<'a> Filter<'a> {
    pub fn new(filter: &'a query::Filter) -> Filter<'a> {
        Filter {
            filter: filter
        }
    }
}

impl<'a> ToJson for Filter<'a> {
    fn to_json(&self) -> Json {
        self.filter.to_json()
    }
}

bucket_agg!(Filter);

/// Filters aggregation
#[derive(Debug)]
pub struct Filters<'a> {
    filters: HashMap<&'a str, &'a query::Filter>
}

impl<'a> Filters<'a> {
    pub fn new(filters: HashMap<&'a str, &'a query::Filter>) -> Filters<'a> {
        Filters {
            filters: filters
        }
    }
}

impl<'a> From<Vec<(&'a str, &'a query::Filter)>> for Filters<'a> {
    fn from(from: Vec<(&'a str, &'a query::Filter)>) -> Filters<'a> {
        let mut filters = HashMap::with_capacity(from.len());
        for (k, v) in from {
            filters.insert(k, v);
        }
        Filters::new(filters)
    }
}

impl<'a> ToJson for Filters<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        let mut inner = BTreeMap::new();
        for (&k, v) in self.filters.iter() {
            inner.insert(k.to_owned(), v.to_json());
        }
        d.insert("filters".to_owned(), Json::Object(inner));
        Json::Object(d)
    }
}

bucket_agg!(Filters);

/// Order - used for some bucketing aggregations to determine the order of
/// buckets
#[derive(Debug)]
pub enum OrderKey<'a> {
    Count,
    Term,
    Expr(&'a str)
}

impl<'a> From<&'a str> for OrderKey<'a> {
    fn from(from: &'a str) -> OrderKey<'a> {
        OrderKey::Expr(from)
    }
}

impl<'a> ToString for OrderKey<'a> {
    fn to_string(&self) -> String {
        match *self {
            OrderKey::Count   => "_count".to_owned(),
            OrderKey::Term    => "_term".to_owned(),
            OrderKey::Expr(e) => e.to_owned()
        }
    }
}

/// Used to define the ordering of buckets in a some bucketted aggregations
///
/// # Examples
///
/// ```
/// use rs_es::operations::search::aggregations::{Order, OrderKey};
///
/// let order1 = Order::asc(OrderKey::Count);
/// let order2 = Order::desc("field_name");
/// ```
///
/// The first will produce a JSON fragment: `{"_count": "asc"}`; the second will
/// produce a JSON fragment: `{"field_name", "desc"}`
#[derive(Debug)]
pub struct Order<'a>(OrderKey<'a>, super::Order);

impl<'a> Order<'a> {
    /// Create an `Order` ascending
    pub fn asc<O: Into<OrderKey<'a>>>(key: O) -> Order<'a> {
        Order(key.into(), super::Order::Asc)
    }

    /// Create an `Order` descending
    pub fn desc<O: Into<OrderKey<'a>>>(key: O) -> Order<'a> {
        Order(key.into(), super::Order::Desc)
    }
}

impl<'a> ToJson for Order<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        d.insert(self.0.to_string(), self.1.to_json());
        Json::Object(d)
    }
}

/// Terms aggregation
#[derive(Debug)]
pub struct Terms<'a> {
    field:      FieldOrScript<'a>,
    size:       Option<u64>,
    shard_size: Option<u64>,
    order:      Option<Order<'a>>
}

impl<'a> Terms<'a> {
    pub fn new<FOS: Into<FieldOrScript<'a>>>(field: FOS) -> Terms<'a> {
        Terms {
            field:      field.into(),
            size:       None,
            shard_size: None,
            order:      None
        }
    }

    add_field!(with_size, size, u64);
    add_field!(with_shard_size, shard_size, u64);
    add_field!(with_order, order, Order<'a>);
}

bucket_agg!(Terms);

impl<'a> ToJson for Terms<'a> {
    fn to_json(&self) -> Json {
        let mut json = BTreeMap::new();
        self.field.add_to_object(&mut json);

        optional_add!(json, self.size, "size");
        optional_add!(json, self.shard_size, "shard_size");
        optional_add!(json, self.order, "order");

        Json::Object(json)
    }
}

/// The set of bucket aggregations
#[derive(Debug)]
pub enum BucketAggregation<'a> {
    Global(Global<'a>),
    Filter(Filter<'a>),
    Filters(Filters<'a>),
    Terms(Terms<'a>)
}

impl<'a> BucketAggregation<'a> {
    fn add_to_object(&self, json: &mut BTreeMap<String, Json>) {
        match self {
            &BucketAggregation::Global(ref g) => {
                json.insert("global".to_owned(), g.to_json());
            },
            &BucketAggregation::Filter(ref filter) => {
                json.insert("filter".to_owned(), filter.to_json());
            },
            &BucketAggregation::Filters(ref filters) => {
                json.insert("filters".to_owned(), filters.to_json());
            },
            &BucketAggregation::Terms(ref terms) => {
                json.insert("terms".to_owned(), terms.to_json());
            }
        }
    }
}

/// Aggregations are either metrics or bucket-based aggregations
#[derive(Debug)]
pub enum Aggregation<'a> {
    /// A metric aggregation (e.g. min)
    Metrics(MetricsAggregation<'a>),

    /// A bucket aggregation, groups data into buckets and optionally applies
    /// sub-aggregations
    Bucket(BucketAggregation<'a>, Option<Aggregations<'a>>)
}

impl<'a> ToJson for Aggregation<'a> {
    fn to_json(&self) -> Json {
        match self {
            &Aggregation::Metrics(ref ma)          => {
                ma.to_json()
            },
            &Aggregation::Bucket(ref ba, ref aggs) => {
                let mut d = BTreeMap::new();
                ba.add_to_object(&mut d);
                match aggs {
                    &Some(ref a) => {
                        d.insert("aggs".to_owned(), a.to_json());
                    },
                    &None        => ()
                }
                Json::Object(d)
            }
        }
    }
}

/// The set of aggregations
///
/// There are many ways of creating aggregations, either standalone or via a
/// conversion trait
#[derive(Debug)]
pub struct Aggregations<'a>(HashMap<&'a str, Aggregation<'a>>);

impl<'a> Aggregations<'a> {
    /// Create an empty-set of aggregations, individual aggregations should be
    /// added via the `add` method
    ///
    /// # Examples
    ///
    /// ```
    /// use rs_es::operations::search::aggregations::{Aggregations, Min};
    ///
    /// let mut aggs = Aggregations::new();
    /// aggs.add("agg_name", Min::new("field_name"));
    /// ```
    pub fn new() -> Aggregations<'a> {
        Aggregations(HashMap::new())
    }

    /// Add an aggregation to the set of aggregations
    pub fn add<A: Into<Aggregation<'a>>>(&mut self, key: &'a str, val: A) {
        self.0.insert(key, val.into());
    }
}

impl<'b> From<Vec<(&'b str, Aggregation<'b>)>> for Aggregations<'b> {
    fn from(from: Vec<(&'b str, Aggregation<'b>)>) -> Aggregations<'b> {
        let mut aggs = Aggregations::new();
        for (name, agg) in from {
            aggs.add(name, agg);
        }
        aggs
    }
}

impl <'a, A: Into<Aggregation<'a>>> From<(&'a str, A)> for Aggregations<'a> {
    fn from(from: (&'a str, A)) -> Aggregations<'a> {
        let mut aggs = Aggregations::new();
        aggs.add(from.0, from.1.into());
        aggs
    }
}

impl<'a> ToJson for Aggregations<'a> {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        for (k, ref v) in self.0.iter() {
            d.insert((*k).to_owned(), v.to_json());
        }
        Json::Object(d)
    }
}

// Result objects

// Metrics result

#[derive(Debug)]
pub struct MinResult {
    pub value: JsonVal
}

impl<'a> From<&'a Json> for MinResult {
    fn from(from: &'a Json) -> MinResult {
        MinResult {
            value: JsonVal::from(from.find("value").expect("No 'value' value"))
        }
    }
}

#[derive(Debug)]
pub struct MaxResult {
    pub value: JsonVal
}

impl<'a> From<&'a Json> for MaxResult {
    fn from(from: &'a Json) -> MaxResult {
        MaxResult {
            value: JsonVal::from(from.find("value").expect("No 'value' value"))
        }
    }
}

#[derive(Debug)]
pub struct SumResult {
    pub value: f64
}

impl<'a> From<&'a Json> for SumResult {
    fn from(from: &'a Json) -> SumResult {
        SumResult {
            value: get_json_f64!(from, "value")
        }
    }
}

#[derive(Debug)]
pub struct AvgResult {
    pub value: f64
}

impl<'a> From<&'a Json> for AvgResult {
    fn from(from: &'a Json) -> AvgResult {
        AvgResult {
            value: get_json_f64!(from, "value")
        }
    }
}

#[derive(Debug)]
pub struct StatsResult {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub sum: f64
}

impl<'a> From<&'a Json> for StatsResult {
    fn from(from: &'a Json) -> StatsResult {
        StatsResult {
            count: get_json_u64!(from, "count"),
            min: get_json_f64!(from, "min"),
            max: get_json_f64!(from, "max"),
            avg: get_json_f64!(from, "avg"),
            sum: get_json_f64!(from, "sum")
        }
    }
}

/// Used by the `ExtendedStatsResult`
#[derive(Debug)]
pub struct Bounds {
    pub upper: f64,
    pub lower: f64
}

impl<'a> From<&'a Json> for Bounds {
    fn from(from: &'a Json) -> Bounds {
        Bounds {
            upper: get_json_f64!(from, "upper"),
            lower: get_json_f64!(from, "lower")
        }
    }
}

#[derive(Debug)]
pub struct ExtendedStatsResult {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub sum: f64,
    pub sum_of_squares: f64,
    pub variance: f64,
    pub std_deviation: f64,
    pub std_deviation_bounds: Bounds
}

impl<'a> From<&'a Json> for ExtendedStatsResult {
    fn from(from: &'a Json) -> ExtendedStatsResult {
        ExtendedStatsResult {
            count: get_json_u64!(from, "count"),
            min: get_json_f64!(from, "min"),
            max: get_json_f64!(from, "max"),
            avg: get_json_f64!(from, "avg"),
            sum: get_json_f64!(from, "sum"),
            sum_of_squares: get_json_f64!(from, "sum_of_squares"),
            variance: get_json_f64!(from, "variance"),
            std_deviation: get_json_f64!(from, "std_deviation"),
            std_deviation_bounds: from.find("std_deviation_bounds")
                .expect("No 'std_deviation_bounds'")
                .into()
        }
    }
}

#[derive(Debug)]
pub struct ValueCountResult {
    pub value: u64
}

impl<'a> From<&'a Json> for ValueCountResult {
    fn from(from: &'a Json) -> ValueCountResult {
        ValueCountResult {
            value: get_json_u64!(from, "value")
        }
    }
}

#[derive(Debug)]
pub struct PercentilesResult {
    pub values: HashMap<String, f64>
}

impl<'a> From<&'a Json> for PercentilesResult {
    fn from(from: &'a Json) -> PercentilesResult {
        let val_obj = get_json_object!(from, "values");
        let mut vals = HashMap::with_capacity(val_obj.len());

        for (k, v) in val_obj.into_iter() {
            vals.insert(k.clone(), v.as_f64().expect("Not numeric value"));
        }

        PercentilesResult {
            values: vals
        }
    }
}

#[derive(Debug)]
pub struct PercentileRanksResult {
    pub values: HashMap<String, f64>
}

impl<'a> From<&'a Json> for PercentileRanksResult {
    fn from(from: &'a Json) -> PercentileRanksResult {
        let val_obj = get_json_object!(from, "values");
        let mut vals = HashMap::with_capacity(val_obj.len());

        for (k, v) in val_obj.into_iter() {
            vals.insert(k.clone(), v.as_f64().expect("Not numeric value"));
        }

        PercentileRanksResult {
            values: vals
        }
    }
}

#[derive(Debug)]
pub struct CardinalityResult {
    pub value: u64
}

impl<'a> From<&'a Json> for CardinalityResult {
    fn from(from: &'a Json) -> CardinalityResult {
        CardinalityResult {
            value: get_json_u64!(from, "value")
        }
    }
}

#[derive(Debug)]
pub struct GeoBoundsResult {
    pub bounds: GeoBox
}

impl<'a> From<&'a Json> for GeoBoundsResult {
    fn from(from: &'a Json) -> GeoBoundsResult {
        GeoBoundsResult {
            bounds: GeoBox::from(from.find("bounds").expect("No 'bounds' field"))
        }
    }
}

#[derive(Debug)]
pub struct ScriptedMetricResult {
    pub value: JsonVal
}

impl<'a> From<&'a Json> for ScriptedMetricResult {
    fn from(from: &'a Json) -> ScriptedMetricResult {
        ScriptedMetricResult {
            value: JsonVal::from(from.find("value").expect("No 'value' field"))
        }
    }
}

// Buckets result

/// Macros for buckets to return a reference to the sub-aggregations
macro_rules! add_aggs_ref {
    () => {
        pub fn aggs_ref<'a>(&'a self) -> Option<&'a AggregationsResult> {
            self.aggs.as_ref()
        }
    }
}

/// Macro to extract sub-aggregations for a bucket aggregation
macro_rules! extract_aggs {
    ($f:ident, $a:ident) => {
        match $a {
            &Some(ref agg) => {
                Some(object_to_result(agg, $f.as_object().expect("Not an object")))
            },
            &None          => None
        }
    }
}

#[derive(Debug)]
pub struct GlobalResult {
    pub doc_count: u64,
    pub aggs: Option<AggregationsResult>
}

impl GlobalResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> GlobalResult {
        GlobalResult {
            doc_count: get_json_u64!(from, "doc_count"),
            aggs: extract_aggs!(from, aggs)
        }
    }

    add_aggs_ref!();
}

#[derive(Debug)]
pub struct FilterResult {
    pub doc_count: u64,
    pub aggs: Option<AggregationsResult>
}

impl FilterResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> FilterResult {
        FilterResult {
            doc_count: get_json_u64!(from, "doc_count"),
            aggs: extract_aggs!(from, aggs)
        }
    }

    add_aggs_ref!();
}

#[derive(Debug)]
pub struct FiltersBucketResult {
    pub doc_count: u64,
    pub aggs: Option<AggregationsResult>
}

impl FiltersBucketResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> FiltersBucketResult {
        FiltersBucketResult {
            doc_count: get_json_u64!(from, "doc_count"),
            aggs: extract_aggs!(from, aggs)
        }
    }

    add_aggs_ref!();
}

#[derive(Debug)]
pub struct FiltersResult {
    pub buckets: HashMap<String, FiltersBucketResult>
}

impl FiltersResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> FiltersResult {
        FiltersResult {
            buckets: from.find("buckets").expect("No buckets")
                .as_object().expect("Not an object")
                .into_iter().map(|(k, v)| {
                    (k.clone(), FiltersBucketResult::from(v, aggs))
                }).collect()
        }
    }
}

#[derive(Debug)]
pub struct TermsBucketResult {
    pub key: JsonVal,
    pub doc_count: u64,
    pub aggs: Option<AggregationsResult>
}

impl TermsBucketResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> TermsBucketResult {
        info!("Creating TermsBucketResult from: {:?} with {:?}", from, aggs);

        TermsBucketResult {
            key: JsonVal::from(from.find("key").expect("No 'key' value")),
            doc_count: get_json_u64!(from, "doc_count"),
            aggs: extract_aggs!(from, aggs)
        }
    }

    add_aggs_ref!();
}

#[derive(Debug)]
pub struct TermsResult {
    pub doc_count_error_upper_bound: u64,
    pub sum_other_doc_count: u64,
    pub buckets: Vec<TermsBucketResult>
}

impl TermsResult {
    fn from(from: &Json, aggs: &Option<Aggregations>) -> TermsResult {
        TermsResult {
            doc_count_error_upper_bound: get_json_u64!(from, "doc_count_error_upper_bound"),
            sum_other_doc_count: get_json_u64!(from, "sum_other_doc_count"),
            buckets: from.find("buckets").expect("No buckets")
                .as_array().expect("Not an array")
                .iter().map(|bucket| {
                    TermsBucketResult::from(bucket, aggs)
                }).collect()
        }
    }
}

/// The result of one specific aggregation
///
/// The data returned varies depending on aggregation type
#[derive(Debug)]
pub enum AggregationResult {
    // Metrics
    Min(MinResult),
    Max(MaxResult),
    Sum(SumResult),
    Avg(AvgResult),
    Stats(StatsResult),
    ExtendedStats(ExtendedStatsResult),
    ValueCount(ValueCountResult),
    Percentiles(PercentilesResult),
    PercentileRanks(PercentileRanksResult),
    Cardinality(CardinalityResult),
    GeoBounds(GeoBoundsResult),
    ScriptedMetric(ScriptedMetricResult),

    // Buckets
    Global(GlobalResult),
    Filter(FilterResult),
    Filters(FiltersResult),
    Terms(TermsResult)
}

/// Macro to implement the various as... functions that return the details of an
/// aggregation for that particular type
macro_rules! agg_as {
    ($n:ident,$t:ident,$rt:ty) => {
        pub fn $n<'a>(&'a self) -> Result<&'a $rt, EsError> {
            match self {
                &AggregationResult::$t(ref res) => Ok(res),
                _                               => {
                    Err(EsError::EsError(format!("Wrong type: {:?}", self)))
                }
            }
        }
    }
}

impl AggregationResult {
    // Metrics
    agg_as!(as_min, Min, MinResult);
    agg_as!(as_max, Max, MaxResult);
    agg_as!(as_sum, Sum, SumResult);
    agg_as!(as_avg, Avg, AvgResult);
    agg_as!(as_stats, Stats, StatsResult);
    agg_as!(as_extended_stats, ExtendedStats, ExtendedStatsResult);
    agg_as!(as_value_count, ValueCount, ValueCountResult);
    agg_as!(as_percentiles, Percentiles, PercentilesResult);
    agg_as!(as_percentile_ranks, PercentileRanks, PercentileRanksResult);
    agg_as!(as_cardinality, Cardinality, CardinalityResult);
    agg_as!(as_geo_bounds, GeoBounds, GeoBoundsResult);
    agg_as!(as_scripted_metric, ScriptedMetric, ScriptedMetricResult);

    // buckets
    agg_as!(as_global, Global, GlobalResult);
    agg_as!(as_filter, Filter, FilterResult);
    agg_as!(as_filters, Filters, FiltersResult);
    agg_as!(as_terms, Terms, TermsResult);
}

#[derive(Debug)]
pub struct AggregationsResult(HashMap<String, AggregationResult>);

/// Loads a Json object of aggregation results into an `AggregationsResult`.
fn object_to_result(aggs: &Aggregations, object: &BTreeMap<String, Json>) -> AggregationsResult {
    let mut ar_map = HashMap::new();

    for (key, val) in aggs.0.iter() {
        let owned_key = (*key).to_owned();
        let json = object.get(&owned_key).expect(&format!("No key: {}", &owned_key));
        ar_map.insert(owned_key, match val {
            &Aggregation::Metrics(ref ma) => {
                match ma {
                    &MetricsAggregation::Min(_) => {
                        AggregationResult::Min(MinResult::from(json))
                    },
                    &MetricsAggregation::Max(_) => {
                        AggregationResult::Max(MaxResult::from(json))
                    },
                    &MetricsAggregation::Sum(_) => {
                        AggregationResult::Sum(SumResult::from(json))
                    },
                    &MetricsAggregation::Avg(_) => {
                        AggregationResult::Avg(AvgResult::from(json))
                    },
                    &MetricsAggregation::Stats(_) => {
                        AggregationResult::Stats(StatsResult::from(json))
                    },
                    &MetricsAggregation::ExtendedStats(_) => {
                        AggregationResult::ExtendedStats(ExtendedStatsResult::from(json))
                    },
                    &MetricsAggregation::ValueCount(_) => {
                        AggregationResult::ValueCount(ValueCountResult::from(json))
                    }
                    &MetricsAggregation::Percentiles(_) => {
                        AggregationResult::Percentiles(PercentilesResult::from(json))
                    },
                    &MetricsAggregation::PercentileRanks(_) => {
                        AggregationResult::PercentileRanks(PercentileRanksResult::from(json))
                    },
                    &MetricsAggregation::Cardinality(_) => {
                        AggregationResult::Cardinality(CardinalityResult::from(json))
                    },
                    &MetricsAggregation::GeoBounds(_) => {
                        AggregationResult::GeoBounds(GeoBoundsResult::from(json))
                    },
                    &MetricsAggregation::ScriptedMetric(_) => {
                        AggregationResult::ScriptedMetric(ScriptedMetricResult::from(json))
                    }
                }
            },
            &Aggregation::Bucket(ref ba, ref aggs) => {
                match ba {
                    &BucketAggregation::Global(_) => {
                        AggregationResult::Global(GlobalResult::from(json, aggs))
                    },
                    &BucketAggregation::Filter(_) => {
                        AggregationResult::Filter(FilterResult::from(json, aggs))
                    },
                    &BucketAggregation::Filters(_) => {
                        AggregationResult::Filters(FiltersResult::from(json, aggs))
                    },
                    &BucketAggregation::Terms(_) => {
                        AggregationResult::Terms(TermsResult::from(json, aggs))
                    }
                }
            }
        });
    }

    info!("Processed aggs - From: {:?}. To: {:?}", object, ar_map);

    AggregationsResult(ar_map)
}

impl AggregationsResult {
    pub fn get<'a>(&'a self, key: &str) -> Result<&'a AggregationResult, EsError> {
        match self.0.get(key) {
            Some(ref agg_res) => Ok(agg_res),
            None              => Err(EsError::EsError(format!("No agg for key: {}",
                                                              key)))
        }
    }

    pub fn from(aggs: &Aggregations, json: &Json) -> AggregationsResult {
        let object = json.find("aggregations")
            .expect("No aggregations")
            .as_object()
            .expect("No aggregations");

        object_to_result(aggs, object)
    }
}