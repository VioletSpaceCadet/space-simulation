//! Columnar Parquet writer for [`MetricsSnapshot`] rows.
//!
//! Mirrors the CSV writer in `sim_core::metrics` but outputs a single
//! `.parquet` file with Zstd compression and schema metadata.

use anyhow::{Context, Result};
use arrow::array::{Float32Builder, Float64Builder, RecordBatch, UInt32Builder, UInt64Builder};
use arrow::datatypes::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use sim_core::MetricsSnapshot;
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
    buffer: RowBuffer,
}

/// Accumulates typed column data before writing a `RecordBatch`.
struct RowBuffer {
    // Fixed columns
    tick: UInt64Builder,
    metrics_version: UInt32Builder,
    total_ore_kg: Float32Builder,
    total_material_kg: Float32Builder,
    total_slag_kg: Float32Builder,
    station_storage_used_pct: Float32Builder,
    ship_cargo_used_pct: Float32Builder,
    ore_lot_count: UInt32Builder,
    avg_material_quality: Float32Builder,
    refinery_active_count: UInt32Builder,
    refinery_starved_count: UInt32Builder,
    refinery_stalled_count: UInt32Builder,
    assembler_active_count: UInt32Builder,
    assembler_stalled_count: UInt32Builder,
    fleet_total: UInt32Builder,
    fleet_idle: UInt32Builder,
    fleet_mining: UInt32Builder,
    fleet_transiting: UInt32Builder,
    fleet_surveying: UInt32Builder,
    fleet_depositing: UInt32Builder,
    scan_sites_remaining: UInt32Builder,
    asteroids_discovered: UInt32Builder,
    asteroids_depleted: UInt32Builder,
    techs_unlocked: UInt32Builder,
    total_scan_data: Float32Builder,
    max_tech_evidence: Float32Builder,
    avg_module_wear: Float32Builder,
    max_module_wear: Float32Builder,
    repair_kits_remaining: UInt32Builder,
    balance: Float64Builder,
    thruster_count: UInt32Builder,
    export_revenue_total: Float64Builder,
    export_count: UInt32Builder,
    power_generated_kw: Float32Builder,
    power_consumed_kw: Float32Builder,
    power_deficit_kw: Float32Builder,
    battery_charge_pct: Float32Builder,
    station_max_temp_mk: UInt32Builder,
    station_avg_temp_mk: UInt32Builder,
    overheat_warning_count: UInt32Builder,
    overheat_critical_count: UInt32Builder,
    heat_wear_multiplier_avg: Float32Builder,

    // Dynamic per-element columns (one builder per element per metric)
    material_kg_cols: Vec<Float32Builder>,
    ore_avg_cols: Vec<Float32Builder>,
    ore_min_cols: Vec<Float32Builder>,
    ore_max_cols: Vec<Float32Builder>,

    row_count: usize,
}

impl RowBuffer {
    fn new(element_count: usize) -> Self {
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
        Self {
            tick: UInt64Builder::new(),
            metrics_version: UInt32Builder::new(),
            total_ore_kg: Float32Builder::new(),
            total_material_kg: Float32Builder::new(),
            total_slag_kg: Float32Builder::new(),
            station_storage_used_pct: Float32Builder::new(),
            ship_cargo_used_pct: Float32Builder::new(),
            ore_lot_count: UInt32Builder::new(),
            avg_material_quality: Float32Builder::new(),
            refinery_active_count: UInt32Builder::new(),
            refinery_starved_count: UInt32Builder::new(),
            refinery_stalled_count: UInt32Builder::new(),
            assembler_active_count: UInt32Builder::new(),
            assembler_stalled_count: UInt32Builder::new(),
            fleet_total: UInt32Builder::new(),
            fleet_idle: UInt32Builder::new(),
            fleet_mining: UInt32Builder::new(),
            fleet_transiting: UInt32Builder::new(),
            fleet_surveying: UInt32Builder::new(),
            fleet_depositing: UInt32Builder::new(),
            scan_sites_remaining: UInt32Builder::new(),
            asteroids_discovered: UInt32Builder::new(),
            asteroids_depleted: UInt32Builder::new(),
            techs_unlocked: UInt32Builder::new(),
            total_scan_data: Float32Builder::new(),
            max_tech_evidence: Float32Builder::new(),
            avg_module_wear: Float32Builder::new(),
            max_module_wear: Float32Builder::new(),
            repair_kits_remaining: UInt32Builder::new(),
            balance: Float64Builder::new(),
            thruster_count: UInt32Builder::new(),
            export_revenue_total: Float64Builder::new(),
            export_count: UInt32Builder::new(),
            power_generated_kw: Float32Builder::new(),
            power_consumed_kw: Float32Builder::new(),
            power_deficit_kw: Float32Builder::new(),
            battery_charge_pct: Float32Builder::new(),
            station_max_temp_mk: UInt32Builder::new(),
            station_avg_temp_mk: UInt32Builder::new(),
            overheat_warning_count: UInt32Builder::new(),
            overheat_critical_count: UInt32Builder::new(),
            heat_wear_multiplier_avg: Float32Builder::new(),
            material_kg_cols,
            ore_avg_cols,
            ore_min_cols,
            ore_max_cols,
            row_count: 0,
        }
    }
}

impl ParquetMetricsWriter {
    /// Create a new Parquet writer at the given path.
    ///
    /// `element_ids` determines the dynamic per-element columns, matching
    /// the same order as the CSV writer.
    pub fn new(path: &Path, element_ids: Vec<String>) -> Result<Self> {
        let schema = Arc::new(build_schema(&element_ids));
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
        Ok(Self {
            writer,
            schema,
            element_ids,
            buffer: RowBuffer::new(element_count),
        })
    }

    /// Append a metrics snapshot. Flushes to disk when the batch buffer is full.
    pub fn write_row(&mut self, snapshot: &MetricsSnapshot) -> Result<()> {
        append_to_buffer(&mut self.buffer, snapshot, &self.element_ids);
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
        let batch = build_record_batch(&mut self.buffer, &self.schema, &self.element_ids)?;
        self.writer.write(&batch).context("writing parquet batch")?;
        let element_count = self.element_ids.len();
        self.buffer = RowBuffer::new(element_count);
        Ok(())
    }
}

/// Build the Arrow schema with fixed + dynamic element columns.
pub(crate) fn build_schema(element_ids: &[String]) -> Schema {
    let mut fields = vec![
        Field::new("tick", DataType::UInt64, false),
        Field::new("metrics_version", DataType::UInt32, false),
        Field::new("total_ore_kg", DataType::Float32, false),
        Field::new("total_material_kg", DataType::Float32, false),
        Field::new("total_slag_kg", DataType::Float32, false),
    ];

    // Dynamic per-element material columns
    for element_id in element_ids {
        fields.push(Field::new(
            format!("material_kg_{element_id}"),
            DataType::Float32,
            false,
        ));
    }

    fields.extend([
        Field::new("station_storage_used_pct", DataType::Float32, false),
        Field::new("ship_cargo_used_pct", DataType::Float32, false),
    ]);

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

    fields.extend([
        Field::new("ore_lot_count", DataType::UInt32, false),
        Field::new("avg_material_quality", DataType::Float32, false),
        Field::new("refinery_active_count", DataType::UInt32, false),
        Field::new("refinery_starved_count", DataType::UInt32, false),
        Field::new("refinery_stalled_count", DataType::UInt32, false),
        Field::new("assembler_active_count", DataType::UInt32, false),
        Field::new("assembler_stalled_count", DataType::UInt32, false),
        Field::new("fleet_total", DataType::UInt32, false),
        Field::new("fleet_idle", DataType::UInt32, false),
        Field::new("fleet_mining", DataType::UInt32, false),
        Field::new("fleet_transiting", DataType::UInt32, false),
        Field::new("fleet_surveying", DataType::UInt32, false),
        Field::new("fleet_depositing", DataType::UInt32, false),
        Field::new("scan_sites_remaining", DataType::UInt32, false),
        Field::new("asteroids_discovered", DataType::UInt32, false),
        Field::new("asteroids_depleted", DataType::UInt32, false),
        Field::new("techs_unlocked", DataType::UInt32, false),
        Field::new("total_scan_data", DataType::Float32, false),
        Field::new("max_tech_evidence", DataType::Float32, false),
        Field::new("avg_module_wear", DataType::Float32, false),
        Field::new("max_module_wear", DataType::Float32, false),
        Field::new("repair_kits_remaining", DataType::UInt32, false),
        Field::new("balance", DataType::Float64, false),
        Field::new("thruster_count", DataType::UInt32, false),
        Field::new("export_revenue_total", DataType::Float64, false),
        Field::new("export_count", DataType::UInt32, false),
        Field::new("power_generated_kw", DataType::Float32, false),
        Field::new("power_consumed_kw", DataType::Float32, false),
        Field::new("power_deficit_kw", DataType::Float32, false),
        Field::new("battery_charge_pct", DataType::Float32, false),
        Field::new("station_max_temp_mk", DataType::UInt32, false),
        Field::new("station_avg_temp_mk", DataType::UInt32, false),
        Field::new("overheat_warning_count", DataType::UInt32, false),
        Field::new("overheat_critical_count", DataType::UInt32, false),
        Field::new("heat_wear_multiplier_avg", DataType::Float32, false),
    ]);

    Schema::new(fields)
}

/// Append a single snapshot's values to the row buffer.
fn append_to_buffer(buf: &mut RowBuffer, snap: &MetricsSnapshot, element_ids: &[String]) {
    buf.tick.append_value(snap.tick);
    buf.metrics_version.append_value(snap.metrics_version);
    buf.total_ore_kg.append_value(snap.total_ore_kg);
    buf.total_material_kg.append_value(snap.total_material_kg);
    buf.total_slag_kg.append_value(snap.total_slag_kg);

    // Dynamic per-element material columns
    for (index, element_id) in element_ids.iter().enumerate() {
        let value = snap
            .per_element_material_kg
            .get(element_id)
            .copied()
            .unwrap_or(0.0);
        buf.material_kg_cols[index].append_value(value);
    }

    buf.station_storage_used_pct
        .append_value(snap.station_storage_used_pct);
    buf.ship_cargo_used_pct
        .append_value(snap.ship_cargo_used_pct);

    // Dynamic per-element ore stats columns
    for (index, element_id) in element_ids.iter().enumerate() {
        let stats = snap.per_element_ore_stats.get(element_id);
        buf.ore_avg_cols[index].append_value(stats.map_or(0.0, |s| s.avg_fraction));
        buf.ore_min_cols[index].append_value(stats.map_or(0.0, |s| s.min_fraction));
        buf.ore_max_cols[index].append_value(stats.map_or(0.0, |s| s.max_fraction));
    }

    buf.ore_lot_count.append_value(snap.ore_lot_count);
    buf.avg_material_quality
        .append_value(snap.avg_material_quality);
    buf.refinery_active_count
        .append_value(snap.refinery_active_count);
    buf.refinery_starved_count
        .append_value(snap.refinery_starved_count);
    buf.refinery_stalled_count
        .append_value(snap.refinery_stalled_count);
    buf.assembler_active_count
        .append_value(snap.assembler_active_count);
    buf.assembler_stalled_count
        .append_value(snap.assembler_stalled_count);
    buf.fleet_total.append_value(snap.fleet_total);
    buf.fleet_idle.append_value(snap.fleet_idle);
    buf.fleet_mining.append_value(snap.fleet_mining);
    buf.fleet_transiting.append_value(snap.fleet_transiting);
    buf.fleet_surveying.append_value(snap.fleet_surveying);
    buf.fleet_depositing.append_value(snap.fleet_depositing);
    buf.scan_sites_remaining
        .append_value(snap.scan_sites_remaining);
    buf.asteroids_discovered
        .append_value(snap.asteroids_discovered);
    buf.asteroids_depleted.append_value(snap.asteroids_depleted);
    buf.techs_unlocked.append_value(snap.techs_unlocked);
    buf.total_scan_data.append_value(snap.total_scan_data);
    buf.max_tech_evidence.append_value(snap.max_tech_evidence);
    buf.avg_module_wear.append_value(snap.avg_module_wear);
    buf.max_module_wear.append_value(snap.max_module_wear);
    buf.repair_kits_remaining
        .append_value(snap.repair_kits_remaining);
    buf.balance.append_value(snap.balance);
    buf.thruster_count.append_value(snap.thruster_count);
    buf.export_revenue_total
        .append_value(snap.export_revenue_total);
    buf.export_count.append_value(snap.export_count);
    buf.power_generated_kw.append_value(snap.power_generated_kw);
    buf.power_consumed_kw.append_value(snap.power_consumed_kw);
    buf.power_deficit_kw.append_value(snap.power_deficit_kw);
    buf.battery_charge_pct.append_value(snap.battery_charge_pct);
    buf.station_max_temp_mk
        .append_value(snap.station_max_temp_mk);
    buf.station_avg_temp_mk
        .append_value(snap.station_avg_temp_mk);
    buf.overheat_warning_count
        .append_value(snap.overheat_warning_count);
    buf.overheat_critical_count
        .append_value(snap.overheat_critical_count);
    buf.heat_wear_multiplier_avg
        .append_value(snap.heat_wear_multiplier_avg);
}

/// Build a `RecordBatch` from the accumulated buffer, consuming builder state.
fn build_record_batch(
    buf: &mut RowBuffer,
    schema: &Arc<Schema>,
    element_ids: &[String],
) -> Result<RecordBatch> {
    let mut columns: Vec<Arc<dyn arrow::array::Array>> = vec![
        Arc::new(buf.tick.finish()),
        Arc::new(buf.metrics_version.finish()),
        Arc::new(buf.total_ore_kg.finish()),
        Arc::new(buf.total_material_kg.finish()),
        Arc::new(buf.total_slag_kg.finish()),
    ];

    for col in &mut buf.material_kg_cols {
        columns.push(Arc::new(col.finish()));
    }

    columns.push(Arc::new(buf.station_storage_used_pct.finish()));
    columns.push(Arc::new(buf.ship_cargo_used_pct.finish()));

    for index in 0..element_ids.len() {
        columns.push(Arc::new(buf.ore_avg_cols[index].finish()));
        columns.push(Arc::new(buf.ore_min_cols[index].finish()));
        columns.push(Arc::new(buf.ore_max_cols[index].finish()));
    }

    columns.extend([
        Arc::new(buf.ore_lot_count.finish()) as Arc<dyn arrow::array::Array>,
        Arc::new(buf.avg_material_quality.finish()),
        Arc::new(buf.refinery_active_count.finish()),
        Arc::new(buf.refinery_starved_count.finish()),
        Arc::new(buf.refinery_stalled_count.finish()),
        Arc::new(buf.assembler_active_count.finish()),
        Arc::new(buf.assembler_stalled_count.finish()),
        Arc::new(buf.fleet_total.finish()),
        Arc::new(buf.fleet_idle.finish()),
        Arc::new(buf.fleet_mining.finish()),
        Arc::new(buf.fleet_transiting.finish()),
        Arc::new(buf.fleet_surveying.finish()),
        Arc::new(buf.fleet_depositing.finish()),
        Arc::new(buf.scan_sites_remaining.finish()),
        Arc::new(buf.asteroids_discovered.finish()),
        Arc::new(buf.asteroids_depleted.finish()),
        Arc::new(buf.techs_unlocked.finish()),
        Arc::new(buf.total_scan_data.finish()),
        Arc::new(buf.max_tech_evidence.finish()),
        Arc::new(buf.avg_module_wear.finish()),
        Arc::new(buf.max_module_wear.finish()),
        Arc::new(buf.repair_kits_remaining.finish()),
        Arc::new(buf.balance.finish()),
        Arc::new(buf.thruster_count.finish()),
        Arc::new(buf.export_revenue_total.finish()),
        Arc::new(buf.export_count.finish()),
        Arc::new(buf.power_generated_kw.finish()),
        Arc::new(buf.power_consumed_kw.finish()),
        Arc::new(buf.power_deficit_kw.finish()),
        Arc::new(buf.battery_charge_pct.finish()),
        Arc::new(buf.station_max_temp_mk.finish()),
        Arc::new(buf.station_avg_temp_mk.finish()),
        Arc::new(buf.overheat_warning_count.finish()),
        Arc::new(buf.overheat_critical_count.finish()),
        Arc::new(buf.heat_wear_multiplier_avg.finish()),
    ]);

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
            refinery_active_count: 2,
            refinery_starved_count: 0,
            refinery_stalled_count: 1,
            assembler_active_count: 1,
            assembler_stalled_count: 0,
            fleet_total: 3,
            fleet_idle: 1,
            fleet_mining: 1,
            fleet_transiting: 1,
            fleet_surveying: 0,
            fleet_depositing: 0,
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
        let mut writer = ParquetMetricsWriter::new(&path, element_ids.clone()).unwrap();
        let snapshots: Vec<MetricsSnapshot> = (0..100).map(make_snapshot).collect();
        for snap in &snapshots {
            writer.write_row(snap).unwrap();
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

        let mut writer = ParquetMetricsWriter::new(&path, element_ids).unwrap();
        writer.write_row(&make_snapshot(0)).unwrap();
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
        let writer = ParquetMetricsWriter::new(&path, element_ids.clone()).unwrap();
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
        let expected_schema = build_schema(&element_ids);
        assert_eq!(
            schema.fields().len(),
            expected_schema.fields().len(),
            "empty file schema should have same column count as build_schema"
        );
    }
}
