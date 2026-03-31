//! Columnar Parquet writer for [`MetricsSnapshot`] rows.
//!
//! Uses [`MetricsSnapshot::fixed_field_descriptors`] and [`MetricsSnapshot::fixed_field_values`]
//! to iterate fields generically, eliminating per-field boilerplate. Dynamic per-element
//! columns are appended after all fixed scalar columns.

use anyhow::{Context, Result};
use arrow::array::{
    Float32Builder, Float64Builder, RecordBatch, StringBuilder, UInt32Builder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use sim_core::{MetricType, MetricValue, MetricsSnapshot, RunScore};
use std::path::Path;
use std::sync::Arc;

/// Batch size for Arrow record batches before flushing to Parquet.
const BATCH_SIZE: usize = 1024;

/// Writes [`MetricsSnapshot`] rows to a Parquet file.
///
/// Dynamic per-element columns are flattened using the same naming
/// convention as the CSV writer: `material_kg_{element_id}`,
/// `ore_avg_{element_id}`, `ore_min_{element_id}`, `ore_max_{element_id}`.
pub struct ParquetMetricsWriter {
    writer: ArrowWriter<std::fs::File>,
    schema: Arc<Schema>,
    element_ids: Vec<String>,
    behavior_types: Vec<String>,
    buffer: RowBuffer,
}

/// Type-erased column builder matching [`MetricType`] variants.
enum ColumnBuilder {
    U32(UInt32Builder),
    U64(UInt64Builder),
    F32(Float32Builder),
    F64(Float64Builder),
}

impl ColumnBuilder {
    fn from_type(metric_type: MetricType) -> Self {
        match metric_type {
            MetricType::U32 => Self::U32(UInt32Builder::new()),
            MetricType::U64 => Self::U64(UInt64Builder::new()),
            MetricType::F32 => Self::F32(Float32Builder::new()),
            MetricType::F64 => Self::F64(Float64Builder::new()),
        }
    }

    fn append(&mut self, value: MetricValue) {
        match (self, value) {
            (Self::U32(b), MetricValue::U32(v)) => b.append_value(v),
            (Self::U64(b), MetricValue::U64(v)) => b.append_value(v),
            (Self::F32(b), MetricValue::F32(v)) => b.append_value(v),
            (Self::F64(b), MetricValue::F64(v)) => b.append_value(v),
            _ => unreachable!("type mismatch between ColumnBuilder and MetricValue"),
        }
    }

    fn finish(&mut self) -> Arc<dyn arrow::array::Array> {
        match self {
            Self::U32(b) => Arc::new(b.finish()),
            Self::U64(b) => Arc::new(b.finish()),
            Self::F32(b) => Arc::new(b.finish()),
            Self::F64(b) => Arc::new(b.finish()),
        }
    }
}

/// Accumulates typed column data before writing a `RecordBatch`.
struct RowBuffer {
    /// One builder per fixed scalar field, same order as `fixed_field_descriptors()`.
    fixed_columns: Vec<ColumnBuilder>,
    /// Dynamic per-element columns (one builder per element per metric).
    material_kg_cols: Vec<Float32Builder>,
    ore_avg_cols: Vec<Float32Builder>,
    ore_min_cols: Vec<Float32Builder>,
    ore_max_cols: Vec<Float32Builder>,
    /// Dynamic per-module-type columns: 3 builders (active, stalled, starved) per type.
    module_active_cols: Vec<UInt32Builder>,
    module_stalled_cols: Vec<UInt32Builder>,
    module_starved_cols: Vec<UInt32Builder>,
    // Score columns (1 f64 composite + 6 f64 dimensions + 1 string threshold)
    score_composite: Float64Builder,
    score_industrial: Float64Builder,
    score_research: Float64Builder,
    score_economic: Float64Builder,
    score_fleet: Float64Builder,
    score_efficiency: Float64Builder,
    score_expansion: Float64Builder,
    score_threshold: StringBuilder,
    row_count: usize,
}

impl RowBuffer {
    fn new(element_count: usize, behavior_type_count: usize) -> Self {
        let fixed_columns = MetricsSnapshot::fixed_field_descriptors()
            .iter()
            .map(|(_, metric_type)| ColumnBuilder::from_type(*metric_type))
            .collect();

        let mut material_kg_cols = Vec::with_capacity(element_count);
        let mut ore_avg_cols = Vec::with_capacity(element_count);
        let mut ore_min_cols = Vec::with_capacity(element_count);
        let mut ore_max_cols = Vec::with_capacity(element_count);
        for _ in 0..element_count {
            material_kg_cols.push(Float32Builder::new());
            ore_avg_cols.push(Float32Builder::new());
            ore_min_cols.push(Float32Builder::new());
            ore_max_cols.push(Float32Builder::new());
        }

        let mut module_active_cols = Vec::with_capacity(behavior_type_count);
        let mut module_stalled_cols = Vec::with_capacity(behavior_type_count);
        let mut module_starved_cols = Vec::with_capacity(behavior_type_count);
        for _ in 0..behavior_type_count {
            module_active_cols.push(UInt32Builder::new());
            module_stalled_cols.push(UInt32Builder::new());
            module_starved_cols.push(UInt32Builder::new());
        }

        Self {
            fixed_columns,
            material_kg_cols,
            ore_avg_cols,
            ore_min_cols,
            ore_max_cols,
            module_active_cols,
            module_stalled_cols,
            module_starved_cols,
            score_composite: Float64Builder::new(),
            score_industrial: Float64Builder::new(),
            score_research: Float64Builder::new(),
            score_economic: Float64Builder::new(),
            score_fleet: Float64Builder::new(),
            score_efficiency: Float64Builder::new(),
            score_expansion: Float64Builder::new(),
            score_threshold: StringBuilder::new(),
            row_count: 0,
        }
    }
}

impl ParquetMetricsWriter {
    /// Create a new Parquet writer at the given path.
    ///
    /// `element_ids` determines the dynamic per-element columns, matching
    /// the same order as the CSV writer.
    pub fn new(path: &Path, element_ids: Vec<String>, behavior_types: Vec<String>) -> Result<Self> {
        let schema = Arc::new(build_schema(&element_ids, &behavior_types));
        let file = std::fs::File::create(path)
            .with_context(|| format!("creating parquet file: {}", path.display()))?;

        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::default()))
            .set_key_value_metadata(Some(vec![parquet::format::KeyValue {
                key: "metrics_version".to_string(),
                value: Some(sim_core::METRICS_VERSION.to_string()),
            }]))
            .build();

        let writer = ArrowWriter::try_new(file, Arc::clone(&schema), Some(props))
            .context("initializing parquet writer")?;

        let element_count = element_ids.len();
        let behavior_type_count = behavior_types.len();
        Ok(Self {
            writer,
            schema,
            element_ids,
            behavior_types,
            buffer: RowBuffer::new(element_count, behavior_type_count),
        })
    }

    /// Append a metrics snapshot with its corresponding score.
    /// Flushes to disk when the batch buffer is full.
    pub fn write_row(&mut self, snapshot: &MetricsSnapshot, score: &RunScore) -> Result<()> {
        append_to_buffer(
            &mut self.buffer,
            snapshot,
            score,
            &self.element_ids,
            &self.behavior_types,
        );
        self.buffer.row_count += 1;

        if self.buffer.row_count >= BATCH_SIZE {
            self.flush_buffer()?;
        }
        Ok(())
    }

    /// Flush remaining rows and close the file.
    pub fn finish(mut self) -> Result<()> {
        if self.buffer.row_count > 0 {
            self.flush_buffer()?;
        }
        self.writer.close().context("closing parquet writer")?;
        Ok(())
    }

    fn flush_buffer(&mut self) -> Result<()> {
        let batch = build_record_batch(
            &mut self.buffer,
            &self.schema,
            &self.element_ids,
            &self.behavior_types,
        )?;
        self.writer.write(&batch).context("writing parquet batch")?;
        self.buffer = RowBuffer::new(self.element_ids.len(), self.behavior_types.len());
        Ok(())
    }
}

/// Build the Arrow schema with fixed + dynamic columns.
///
/// Column order (v11): fixed scalar fields, per-element columns, per-module-type columns.
pub(crate) fn build_schema(element_ids: &[String], behavior_types: &[String]) -> Schema {
    let mut fields: Vec<Field> = MetricsSnapshot::fixed_field_descriptors()
        .iter()
        .map(|(name, metric_type)| {
            let data_type = match metric_type {
                MetricType::U32 => DataType::UInt32,
                MetricType::U64 => DataType::UInt64,
                MetricType::F32 => DataType::Float32,
                MetricType::F64 => DataType::Float64,
            };
            Field::new(*name, data_type, false)
        })
        .collect();

    // Dynamic per-element material columns
    for element_id in element_ids {
        fields.push(Field::new(
            format!("material_kg_{element_id}"),
            DataType::Float32,
            false,
        ));
    }

    // Dynamic per-element ore stats columns
    for element_id in element_ids {
        fields.push(Field::new(
            format!("ore_avg_{element_id}"),
            DataType::Float32,
            false,
        ));
        fields.push(Field::new(
            format!("ore_min_{element_id}"),
            DataType::Float32,
            false,
        ));
        fields.push(Field::new(
            format!("ore_max_{element_id}"),
            DataType::Float32,
            false,
        ));
    }

    // Dynamic per-module-type columns
    for bt in behavior_types {
        fields.push(Field::new(format!("{bt}_active"), DataType::UInt32, false));
        fields.push(Field::new(format!("{bt}_stalled"), DataType::UInt32, false));
        fields.push(Field::new(format!("{bt}_starved"), DataType::UInt32, false));
    }

    // Score columns
    fields.push(Field::new("score_composite", DataType::Float64, false));
    fields.push(Field::new("score_industrial", DataType::Float64, false));
    fields.push(Field::new("score_research", DataType::Float64, false));
    fields.push(Field::new("score_economic", DataType::Float64, false));
    fields.push(Field::new("score_fleet", DataType::Float64, false));
    fields.push(Field::new("score_efficiency", DataType::Float64, false));
    fields.push(Field::new("score_expansion", DataType::Float64, false));
    fields.push(Field::new("score_threshold", DataType::Utf8, false));

    Schema::new(fields)
}

/// Append a single snapshot's values and score to the row buffer.
fn append_to_buffer(
    buf: &mut RowBuffer,
    snap: &MetricsSnapshot,
    score: &RunScore,
    element_ids: &[String],
    behavior_types: &[String],
) {
    // Fixed scalar columns — iterate field values in lockstep with builders.
    for (builder, (_, value)) in buf.fixed_columns.iter_mut().zip(snap.fixed_field_values()) {
        builder.append(value);
    }

    // Dynamic per-element material columns
    for (index, element_id) in element_ids.iter().enumerate() {
        let value = snap
            .per_element_material_kg
            .get(element_id)
            .copied()
            .unwrap_or(0.0);
        buf.material_kg_cols[index].append_value(value);
    }

    // Dynamic per-element ore stats columns
    for (index, element_id) in element_ids.iter().enumerate() {
        let stats = snap.per_element_ore_stats.get(element_id);
        buf.ore_avg_cols[index].append_value(stats.map_or(0.0, |s| s.avg_fraction));
        buf.ore_min_cols[index].append_value(stats.map_or(0.0, |s| s.min_fraction));
        buf.ore_max_cols[index].append_value(stats.map_or(0.0, |s| s.max_fraction));
    }

    // Dynamic per-module-type columns
    for (index, bt) in behavior_types.iter().enumerate() {
        let metrics = snap.per_module_metrics.get(bt);
        buf.module_active_cols[index].append_value(metrics.map_or(0, |m| m.active));
        buf.module_stalled_cols[index].append_value(metrics.map_or(0, |m| m.stalled));
        buf.module_starved_cols[index].append_value(metrics.map_or(0, |m| m.starved));
    }

    // Score columns
    let dim = |id: &str| -> f64 { score.dimensions.get(id).map_or(0.0, |d| d.normalized) };
    buf.score_composite.append_value(score.composite);
    buf.score_industrial.append_value(dim("industrial_output"));
    buf.score_research.append_value(dim("research_progress"));
    buf.score_economic.append_value(dim("economic_health"));
    buf.score_fleet.append_value(dim("fleet_operations"));
    buf.score_efficiency.append_value(dim("efficiency"));
    buf.score_expansion.append_value(dim("expansion"));
    buf.score_threshold.append_value(&score.threshold);
}

/// Build a `RecordBatch` from the accumulated buffer, consuming builder state.
fn build_record_batch(
    buf: &mut RowBuffer,
    schema: &Arc<Schema>,
    element_ids: &[String],
    behavior_types: &[String],
) -> Result<RecordBatch> {
    // Fixed scalar columns
    let mut columns: Vec<Arc<dyn arrow::array::Array>> = buf
        .fixed_columns
        .iter_mut()
        .map(ColumnBuilder::finish)
        .collect();

    // Dynamic per-element columns
    for col in &mut buf.material_kg_cols {
        columns.push(Arc::new(col.finish()));
    }
    for index in 0..element_ids.len() {
        columns.push(Arc::new(buf.ore_avg_cols[index].finish()));
        columns.push(Arc::new(buf.ore_min_cols[index].finish()));
        columns.push(Arc::new(buf.ore_max_cols[index].finish()));
    }

    // Dynamic per-module-type columns
    for index in 0..behavior_types.len() {
        columns.push(Arc::new(buf.module_active_cols[index].finish()));
        columns.push(Arc::new(buf.module_stalled_cols[index].finish()));
        columns.push(Arc::new(buf.module_starved_cols[index].finish()));
    }

    // Score columns
    columns.push(Arc::new(buf.score_composite.finish()));
    columns.push(Arc::new(buf.score_industrial.finish()));
    columns.push(Arc::new(buf.score_research.finish()));
    columns.push(Arc::new(buf.score_economic.finish()));
    columns.push(Arc::new(buf.score_fleet.finish()));
    columns.push(Arc::new(buf.score_efficiency.finish()));
    columns.push(Arc::new(buf.score_expansion.finish()));
    columns.push(Arc::new(buf.score_threshold.finish()));

    RecordBatch::try_new(Arc::clone(schema), columns).context("building record batch")
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{AsArray, Float32Array, Float64Array, UInt32Array, UInt64Array};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::file::reader::FileReader;
    use sim_core::{MetricsSnapshot, OreElementStats, METRICS_VERSION};
    use std::collections::BTreeMap;

    /// Build a test `MetricsSnapshot` with deterministic values derived from `index`.
    fn make_snapshot(index: u64) -> MetricsSnapshot {
        let mut per_element_material_kg = BTreeMap::new();
        per_element_material_kg.insert("Fe".to_string(), 100.0 + index as f32);
        per_element_material_kg.insert("H2O".to_string(), 50.0 + index as f32);

        let mut per_element_ore_stats = BTreeMap::new();
        per_element_ore_stats.insert(
            "Fe".to_string(),
            OreElementStats {
                avg_fraction: 0.5 + (index as f32) * 0.001,
                min_fraction: 0.3,
                max_fraction: 0.7,
            },
        );
        per_element_ore_stats.insert(
            "H2O".to_string(),
            OreElementStats {
                avg_fraction: 0.2,
                min_fraction: 0.1,
                max_fraction: 0.3,
            },
        );

        MetricsSnapshot {
            tick: index * 10,
            metrics_version: METRICS_VERSION,
            total_ore_kg: 1000.0 + index as f32,
            total_material_kg: 500.0 + index as f32,
            total_slag_kg: 200.0 + index as f32,
            per_element_material_kg,
            station_storage_used_pct: 0.45,
            ship_cargo_used_pct: 0.3,
            per_element_ore_stats,
            ore_lot_count: 5 + index as u32,
            avg_material_quality: 0.85,
            per_module_metrics: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "processor".to_string(),
                    sim_core::ModuleStatusMetrics {
                        active: 2,
                        stalled: 1,
                        starved: 0,
                    },
                );
                m.insert(
                    "assembler".to_string(),
                    sim_core::ModuleStatusMetrics {
                        active: 1,
                        stalled: 0,
                        starved: 0,
                    },
                );
                m
            },
            fleet_total: 3,
            fleet_idle: 1,
            fleet_mining: 1,
            fleet_transiting: 1,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            fleet_propellant_kg: 0.0,
            fleet_propellant_pct: 0.0,
            propellant_consumed_total: 0.0,
            scan_sites_remaining: 10,
            asteroids_discovered: 5 + index as u32,
            asteroids_depleted: 2,
            techs_unlocked: 3,
            total_scan_data: 150.0 + index as f32,
            max_tech_evidence: 0.9,
            avg_module_wear: 0.15,
            max_module_wear: 0.35,
            repair_kits_remaining: 8,
            balance: 1_000_000.0 + index as f64 * 1000.0,
            crew_salary_per_hour: 100.0,
            thruster_count: 2,
            export_revenue_total: 50_000.0 + index as f64 * 500.0,
            export_count: 10 + index as u32,
            power_generated_kw: 100.0,
            power_consumed_kw: 75.0,
            power_deficit_kw: 0.0,
            battery_charge_pct: 0.95,
            station_max_temp_mk: 350_000,
            station_avg_temp_mk: 300_000,
            overheat_warning_count: 0,
            overheat_critical_count: 0,
            heat_wear_multiplier_avg: 1.0,
        }
    }

    #[test]
    fn round_trip_100_rows() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("metrics.parquet");
        let element_ids = vec!["Fe".to_string(), "H2O".to_string()];

        // Write 100 rows
        let mut writer = ParquetMetricsWriter::new(
            &path,
            element_ids.clone(),
            vec!["processor".to_string(), "assembler".to_string()],
        )
        .unwrap();
        let snapshots: Vec<MetricsSnapshot> = (0..100).map(make_snapshot).collect();
        let default_score = RunScore::default();
        for snap in &snapshots {
            writer.write_row(snap, &default_score).unwrap();
        }
        writer.finish().unwrap();

        // Read back
        let file = std::fs::File::open(&path).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();

        let mut total_rows = 0usize;
        for batch_result in reader {
            let batch = batch_result.unwrap();
            let row_count = batch.num_rows();

            // Verify tick values
            let ticks = batch
                .column_by_name("tick")
                .unwrap()
                .as_primitive::<arrow::datatypes::UInt64Type>();
            for row in 0..row_count {
                let expected_tick = (total_rows + row) as u64 * 10;
                assert_eq!(
                    ticks.value(row),
                    expected_tick,
                    "tick mismatch at row {}",
                    total_rows + row
                );
            }

            // Verify f32 field
            let ore = batch.column_by_name("total_ore_kg").unwrap();
            let ore_values: &Float32Array = ore.as_any().downcast_ref().unwrap();
            for row in 0..row_count {
                let expected = 1000.0 + (total_rows + row) as f32;
                assert!(
                    (ore_values.value(row) - expected).abs() < f32::EPSILON,
                    "total_ore_kg mismatch at row {}",
                    total_rows + row
                );
            }

            // Verify f64 field (balance)
            let balance = batch.column_by_name("balance").unwrap();
            let balance_values: &Float64Array = balance.as_any().downcast_ref().unwrap();
            for row in 0..row_count {
                let expected = 1_000_000.0 + (total_rows + row) as f64 * 1000.0;
                assert!(
                    (balance_values.value(row) - expected).abs() < f64::EPSILON,
                    "balance mismatch at row {}",
                    total_rows + row
                );
            }

            // Verify dynamic element column
            let fe_material = batch.column_by_name("material_kg_Fe").unwrap();
            let fe_values: &Float32Array = fe_material.as_any().downcast_ref().unwrap();
            for row in 0..row_count {
                let expected = 100.0 + (total_rows + row) as f32;
                assert!(
                    (fe_values.value(row) - expected).abs() < f32::EPSILON,
                    "material_kg_Fe mismatch at row {}",
                    total_rows + row
                );
            }

            // Verify u32 field
            let fleet = batch.column_by_name("fleet_total").unwrap();
            let fleet_values: &UInt32Array = fleet.as_any().downcast_ref().unwrap();
            for row in 0..row_count {
                assert_eq!(
                    fleet_values.value(row),
                    3,
                    "fleet_total mismatch at row {}",
                    total_rows + row
                );
            }

            total_rows += row_count;
        }

        assert_eq!(total_rows, 100, "expected 100 rows, got {total_rows}");
    }

    #[test]
    fn schema_version_in_metadata() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("metrics.parquet");
        let element_ids = vec!["Fe".to_string()];

        let bt = vec!["processor".to_string(), "assembler".to_string()];
        let mut writer = ParquetMetricsWriter::new(&path, element_ids, bt).unwrap();
        writer
            .write_row(&make_snapshot(0), &RunScore::default())
            .unwrap();
        writer.finish().unwrap();

        // Read file metadata
        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::file::reader::SerializedFileReader::new(file).unwrap();
        let metadata = reader.metadata().file_metadata();
        let kv = metadata
            .key_value_metadata()
            .expect("expected key-value metadata");

        let version_entry = kv
            .iter()
            .find(|entry| entry.key == "metrics_version")
            .expect("metrics_version key not found in metadata");

        assert_eq!(
            version_entry.value.as_deref(),
            Some(&*METRICS_VERSION.to_string()),
            "metrics_version should match METRICS_VERSION constant"
        );
    }

    #[test]
    fn empty_file_is_valid_parquet() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("metrics.parquet");
        let element_ids = vec!["Fe".to_string(), "H2O".to_string()];

        // Write 0 rows
        let writer = ParquetMetricsWriter::new(
            &path,
            element_ids.clone(),
            vec!["processor".to_string(), "assembler".to_string()],
        )
        .unwrap();
        writer.finish().unwrap();

        // Verify the file exists and is valid Parquet
        let file = std::fs::File::open(&path).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();

        let mut total_rows = 0usize;
        for batch_result in reader {
            total_rows += batch_result.unwrap().num_rows();
        }
        assert_eq!(total_rows, 0, "empty file should have 0 rows");

        // Verify schema has correct column count
        let file2 = std::fs::File::open(&path).unwrap();
        let reader2 = ParquetRecordBatchReaderBuilder::try_new(file2).unwrap();
        let schema = reader2.schema();
        let bt = vec!["processor".to_string(), "assembler".to_string()];
        let expected_schema = build_schema(&element_ids, &bt);
        assert_eq!(
            schema.fields().len(),
            expected_schema.fields().len(),
            "empty file schema should have same column count as build_schema"
        );
    }
}
