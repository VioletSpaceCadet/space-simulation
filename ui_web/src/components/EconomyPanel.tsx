import { useCallback, useEffect, useMemo, useState } from 'react';

import type { PricingTable, SimEvent, SimSnapshot, TradeItemSpec } from '../types';
import { formatCurrency } from '../utils';

type ItemCategory = 'Material' | 'Component' | 'Module'

interface CategorizedItem {
  key: string
  label: string
  category: ItemCategory
  basePrice: number
  importable: boolean
  exportable: boolean
}

function categorizePricingItems(pricing: PricingTable): CategorizedItem[] {
  const materials = new Set(['Fe', 'Si', 'He']);
  const items: CategorizedItem[] = [];

  for (const [key, entry] of Object.entries(pricing.items)) {
    if (key === 'ore' || key === 'slag') {continue;}

    let category: ItemCategory;
    if (materials.has(key)) {
      category = 'Material';
    } else if (key.startsWith('module_')) {
      category = 'Module';
    } else {
      category = 'Component';
    }

    items.push({
      key,
      label: key.replace(/_/g, ' '),
      category,
      basePrice: entry.base_price_per_unit,
      importable: entry.importable,
      exportable: entry.exportable,
    });
  }

  return items;
}

function buildTradeItemSpec(category: ItemCategory, itemKey: string, quantity: number): TradeItemSpec {
  switch (category) {
    case 'Material':
      return { Material: { element: itemKey, kg: quantity } };
    case 'Component':
      return { Component: { component_id: itemKey, count: quantity } };
    case 'Module':
      return { Module: { module_def_id: itemKey } };
  }
}

function estimateWeight(category: ItemCategory, quantity: number): number {
  switch (category) {
    case 'Material':
      return quantity;
    case 'Component':
      return quantity * 5;
    case 'Module':
      return 500;
  }
}

interface Props {
  snapshot: SimSnapshot | null
  events: SimEvent[]
}

export function EconomyPanel({ snapshot, events }: Props) {
  const [pricing, setPricing] = useState<PricingTable | null>(null);
  const [importCategory, setImportCategory] = useState<ItemCategory>('Material');
  const [importItem, setImportItem] = useState('');
  const [importQuantity, setImportQuantity] = useState(1);
  const [exportCategory, setExportCategory] = useState<ItemCategory>('Material');
  const [exportItem, setExportItem] = useState('');
  const [exportQuantity, setExportQuantity] = useState(1);
  const [importStatus, setImportStatus] = useState<'idle' | 'sending' | 'sent' | 'error'>('idle');
  const [exportStatus, setExportStatus] = useState<'idle' | 'sending' | 'sent' | 'error'>('idle');

  useEffect(() => {
    fetch('/api/v1/pricing')
      .then((response) => {
        if (!response.ok) {throw new Error('Failed to fetch pricing');}
        return response.json();
      })
      .then((data: PricingTable) => setPricing(data))
      .catch(() => {});
  }, []);

  const categorizedItems = useMemo(() => {
    if (!pricing) {return [];}
    return categorizePricingItems(pricing);
  }, [pricing]);

  const importItems = useMemo(
    () => categorizedItems.filter((item) => item.category === importCategory && item.importable),
    [categorizedItems, importCategory],
  );

  const exportItems = useMemo(
    () => categorizedItems.filter((item) => item.category === exportCategory && item.exportable),
    [categorizedItems, exportCategory],
  );

  // Category change handlers: reset selected item and quantity in one pass
  const handleImportCategoryChange = (category: ItemCategory) => {
    setImportCategory(category);
    const items = categorizedItems.filter((item) => item.category === category && item.importable);
    setImportItem(items[0]?.key ?? '');
    if (category === 'Module') {setImportQuantity(1);}
  };

  const handleExportCategoryChange = (category: ItemCategory) => {
    setExportCategory(category);
    const items = categorizedItems.filter((item) => item.category === category && item.exportable);
    setExportItem(items[0]?.key ?? '');
    if (category === 'Module') {setExportQuantity(1);}
  };

  const importCost = useMemo(() => {
    if (!pricing || !importItem) {return 0;}
    const entry = pricing.items[importItem];
    if (!entry) {return 0;}
    const weight = estimateWeight(importCategory, importQuantity);
    return entry.base_price_per_unit * importQuantity + pricing.import_surcharge_per_kg * weight;
  }, [pricing, importItem, importCategory, importQuantity]);

  const exportRevenue = useMemo(() => {
    if (!pricing || !exportItem) {return 0;}
    const entry = pricing.items[exportItem];
    if (!entry) {return 0;}
    const weight = estimateWeight(exportCategory, exportQuantity);
    return entry.base_price_per_unit * exportQuantity - pricing.export_surcharge_per_kg * weight;
  }, [pricing, exportItem, exportCategory, exportQuantity]);

  const stationId = useMemo(() => {
    if (!snapshot) {return null;}
    const stationIds = Object.keys(snapshot.stations);
    return stationIds[0] ?? null;
  }, [snapshot]);

  const sendCommand = useCallback(
    async (commandType: 'Import' | 'Export', category: ItemCategory, itemKey: string, quantity: number) => {
      if (!stationId) {return;}
      const itemSpec = buildTradeItemSpec(category, itemKey, quantity);
      const command = {
        [commandType]: {
          station_id: stationId,
          item_spec: itemSpec,
        },
      };

      const response = await fetch('/api/v1/command', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command }),
      });
      if (!response.ok) {throw new Error('Command failed');}
    },
    [stationId],
  );

  const handleImport = useCallback(async () => {
    if (!importItem) {return;}
    setImportStatus('sending');
    try {
      await sendCommand('Import', importCategory, importItem, importQuantity);
      setImportStatus('sent');
      setTimeout(() => setImportStatus('idle'), 1500);
    } catch {
      setImportStatus('error');
      setTimeout(() => setImportStatus('idle'), 2000);
    }
  }, [sendCommand, importCategory, importItem, importQuantity]);

  const handleExport = useCallback(async () => {
    if (!exportItem) {return;}
    setExportStatus('sending');
    try {
      await sendCommand('Export', exportCategory, exportItem, exportQuantity);
      setExportStatus('sent');
      setTimeout(() => setExportStatus('idle'), 1500);
    } catch {
      setExportStatus('error');
      setTimeout(() => setExportStatus('idle'), 2000);
    }
  }, [sendCommand, exportCategory, exportItem, exportQuantity]);

  const tradeEvents = useMemo(() => {
    return events
      .filter((event) => {
        const key = Object.keys(event.event)[0];
        return key === 'ItemImported' || key === 'ItemExported';
      })
      .slice(-20)
      .reverse();
  }, [events]);

  const selectClass =
    'bg-panel border border-edge rounded-sm px-1.5 py-1 text-[11px] text-dim focus:outline-none focus:border-accent/50 w-full';
  const inputClass =
    'bg-panel border border-edge rounded-sm px-1.5 py-1 text-[11px] text-dim focus:outline-none focus:border-accent/50 w-20';
  const buttonBaseClass =
    'px-2.5 py-1 rounded-sm text-[10px] uppercase tracking-widest transition-colors cursor-pointer border';

  return (
    <div className="overflow-y-auto flex-1">
      {/* Balance */}
      <div className="mb-3 pb-2 border-b border-surface">
        <div className="text-[10px] uppercase tracking-widest text-label mb-0.5">Balance</div>
        <div className="text-accent font-bold text-lg">
          {snapshot ? formatCurrency(snapshot.balance) : '--'}
        </div>
      </div>

      {!pricing ? (
        <div className="text-faint italic text-[11px]">Loading pricing data...</div>
      ) : (
        <>
          {/* Import */}
          <div className="mb-3 pb-2 border-b border-surface">
            <div className="text-[10px] uppercase tracking-widest text-label mb-1.5">Import</div>
            <div className="flex flex-col gap-1.5">
              <select
                value={importCategory}
                onChange={(event) => handleImportCategoryChange(event.target.value as ItemCategory)}
                className={selectClass}
              >
                <option value="Material">Material</option>
                <option value="Component">Component</option>
                <option value="Module">Module</option>
              </select>
              <select
                value={importItem}
                onChange={(event) => setImportItem(event.target.value)}
                className={selectClass}
              >
                {importItems.map((item) => (
                  <option key={item.key} value={item.key}>
                    {item.label} ({formatCurrency(item.basePrice)}/unit)
                  </option>
                ))}
              </select>
              {importCategory !== 'Module' && (
                <div className="flex items-center gap-1.5">
                  <label className="text-[10px] text-muted">
                    {importCategory === 'Material' ? 'kg' : 'qty'}:
                  </label>
                  <input
                    type="number"
                    min={1}
                    value={importQuantity}
                    onChange={(event) => setImportQuantity(Math.max(1, Number(event.target.value)))}
                    className={inputClass}
                  />
                </div>
              )}
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted">
                  Cost: <span className="text-accent">{formatCurrency(importCost)}</span>
                </span>
                <button
                  type="button"
                  onClick={handleImport}
                  disabled={!stationId || importStatus === 'sending' || !importItem}
                  className={`${buttonBaseClass} ${
                    importStatus === 'sent'
                      ? 'border-online/40 text-online'
                      : importStatus === 'error'
                        ? 'border-offline/40 text-offline'
                        : 'border-edge text-muted hover:text-dim hover:border-dim'
                  } disabled:opacity-50 disabled:cursor-not-allowed`}
                >
                  {importStatus === 'sending'
                    ? 'Sending...'
                    : importStatus === 'sent'
                      ? 'Sent'
                      : importStatus === 'error'
                        ? 'Failed'
                        : 'Import'}
                </button>
              </div>
            </div>
          </div>

          {/* Export */}
          <div className="mb-3 pb-2 border-b border-surface">
            <div className="text-[10px] uppercase tracking-widest text-label mb-1.5">Export</div>
            <div className="flex flex-col gap-1.5">
              <select
                value={exportCategory}
                onChange={(event) => handleExportCategoryChange(event.target.value as ItemCategory)}
                className={selectClass}
              >
                <option value="Material">Material</option>
                <option value="Component">Component</option>
                <option value="Module">Module</option>
              </select>
              <select
                value={exportItem}
                onChange={(event) => setExportItem(event.target.value)}
                className={selectClass}
              >
                {exportItems.map((item) => (
                  <option key={item.key} value={item.key}>
                    {item.label} ({formatCurrency(item.basePrice)}/unit)
                  </option>
                ))}
              </select>
              {exportCategory !== 'Module' && (
                <div className="flex items-center gap-1.5">
                  <label className="text-[10px] text-muted">
                    {exportCategory === 'Material' ? 'kg' : 'qty'}:
                  </label>
                  <input
                    type="number"
                    min={1}
                    value={exportQuantity}
                    onChange={(event) => setExportQuantity(Math.max(1, Number(event.target.value)))}
                    className={inputClass}
                  />
                </div>
              )}
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted">
                  Revenue: <span className="text-online">{formatCurrency(Math.max(0, exportRevenue))}</span>
                </span>
                <button
                  type="button"
                  onClick={handleExport}
                  disabled={!stationId || exportStatus === 'sending' || !exportItem}
                  className={`${buttonBaseClass} ${
                    exportStatus === 'sent'
                      ? 'border-online/40 text-online'
                      : exportStatus === 'error'
                        ? 'border-offline/40 text-offline'
                        : 'border-edge text-muted hover:text-dim hover:border-dim'
                  } disabled:opacity-50 disabled:cursor-not-allowed`}
                >
                  {exportStatus === 'sending'
                    ? 'Sending...'
                    : exportStatus === 'sent'
                      ? 'Sent'
                      : exportStatus === 'error'
                        ? 'Failed'
                        : 'Export'}
                </button>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Recent Transactions */}
      <div>
        <div className="text-[10px] uppercase tracking-widest text-label mb-1.5">Recent Transactions</div>
        {tradeEvents.length === 0 ? (
          <div className="text-faint italic text-[11px]">no transactions yet</div>
        ) : (
          <div className="flex flex-col gap-0.5">
            {tradeEvents.map((event) => {
              const eventKey = Object.keys(event.event)[0];
              const data = event.event[eventKey] as Record<string, unknown>;
              const isImport = eventKey === 'ItemImported';
              const amount = isImport ? (data.cost as number) : (data.revenue as number);
              const itemSpec = data.item_spec as Record<string, unknown>;
              const specKey = Object.keys(itemSpec)[0];
              const specData = itemSpec[specKey] as Record<string, unknown>;

              let itemLabel = specKey;
              if (specKey === 'Material') {
                itemLabel = `${specData.element} (${specData.kg}kg)`;
              } else if (specKey === 'Component') {
                itemLabel = `${(specData.component_id as string).replace(/_/g, ' ')} x${specData.count}`;
              } else if (specKey === 'Module') {
                itemLabel = (specData.module_def_id as string).replace(/_/g, ' ');
              }

              return (
                <div key={event.id} className="flex items-center justify-between py-0.5 text-[11px]">
                  <span className="text-dim">
                    <span className="text-faint">t{event.tick}</span>{' '}
                    <span className={isImport ? 'text-accent' : 'text-online'}>
                      {isImport ? 'IMP' : 'EXP'}
                    </span>{' '}
                    {itemLabel}
                  </span>
                  <span className={isImport ? 'text-offline' : 'text-online'}>
                    {isImport ? '-' : '+'}
                    {formatCurrency(amount)}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
