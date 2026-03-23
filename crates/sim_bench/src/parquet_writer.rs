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
                value: Some("9".to_string()),
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
pub fn build_schema(element_ids: &[String]) -> Schema {
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
