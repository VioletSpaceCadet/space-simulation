import type { AsteroidState, ComponentItem, MaterialItem, ModuleKindState, ResearchState, ScanSite, ShipState, SimEvent, SlagItem, StationState, TaskState } from '../types'

function buildTaskStub(taskKind: string, target: string | null, tick: number): TaskState {
  const kindMap: Record<string, Record<string, unknown>> = {
    Survey: target ? { Survey: { site: target } } : { Idle: {} },
    DeepScan: target ? { DeepScan: { asteroid: target } } : { Idle: {} },
    Mine: target ? { Mine: { asteroid: target, duration_ticks: 0 } } : { Idle: {} },
    Deposit: target ? { Deposit: { station: target } } : { Idle: {} },
    Transit: target ? { Transit: { destination: target, total_ticks: 0 } } : { Idle: {} },
  }
  return {
    kind: (kindMap[taskKind] ?? { Idle: {} }) as TaskState['kind'],
    started_tick: tick,
    eta_tick: 0,
  }
}

export function applyEvents(
  asteroids: Record<string, AsteroidState>,
  ships: Record<string, ShipState>,
  stations: Record<string, StationState>,
  research: ResearchState,
  scanSites: ScanSite[],
  events: SimEvent[],
): {
  asteroids: Record<string, AsteroidState>
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  research: ResearchState
  scanSites: ScanSite[]
} {
  let updatedAsteroids = { ...asteroids }
  let updatedShips = { ...ships }
  let updatedStations = { ...stations }
  let updatedResearch = research
  const updatedScanSites = [...scanSites]

  for (const evt of events) {
    const e = evt.event
    const eventKey = Object.keys(e)[0]
    const event = e[eventKey] as Record<string, unknown>

    switch (eventKey) {
      case 'AsteroidDiscovered': {
        const { asteroid_id, location_node } = event as { asteroid_id: string; location_node: string }
        if (!updatedAsteroids[asteroid_id]) {
          updatedAsteroids = {
            ...updatedAsteroids,
            [asteroid_id]: {
              id: asteroid_id,
              location_node,
              anomaly_tags: [],
              // mass_kg intentionally omitted — unknown until snapshot or OreMined event
              knowledge: { tag_beliefs: [], composition: null },
            },
          }
        }
        break
      }

      case 'OreMined': {
        const { ship_id, asteroid_id, ore_lot, asteroid_remaining_kg } = event as {
          ship_id: string
          asteroid_id: string
          ore_lot: ShipState['inventory'][number]
          asteroid_remaining_kg: number
        }
        if (asteroid_remaining_kg <= 0) {
          updatedAsteroids = Object.fromEntries(
            Object.entries(updatedAsteroids).filter(([id]) => id !== asteroid_id)
          )
        } else if (updatedAsteroids[asteroid_id]) {
          updatedAsteroids = {
            ...updatedAsteroids,
            [asteroid_id]: { ...updatedAsteroids[asteroid_id], mass_kg: asteroid_remaining_kg },
          }
        }
        if (updatedShips[ship_id]) {
          updatedShips = {
            ...updatedShips,
            [ship_id]: {
              ...updatedShips[ship_id],
              inventory: [...updatedShips[ship_id].inventory, ore_lot],
            },
          }
        }
        break
      }

      case 'OreDeposited': {
        const { ship_id, station_id, items } = event as {
          ship_id: string
          station_id: string
          items: StationState['inventory']
        }
        if (updatedShips[ship_id]) {
          updatedShips = {
            ...updatedShips,
            [ship_id]: { ...updatedShips[ship_id], inventory: [] },
          }
        }
        if (updatedStations[station_id]) {
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...updatedStations[station_id],
              inventory: [...updatedStations[station_id].inventory, ...items],
            },
          }
        }
        break
      }

      case 'ModuleInstalled': {
        const { station_id, module_id, module_item_id, module_def_id } = event as {
          station_id: string
          module_id: string
          module_item_id: string
          module_def_id: string
        }
        if (updatedStations[station_id]) {
          const station = updatedStations[station_id]
          const kindState: ModuleKindState = module_def_id.includes('maintenance')
            ? { Maintenance: { ticks_since_last_run: 0 } }
            : module_def_id.includes('assembler')
              ? { Assembler: { ticks_since_last_run: 0, stalled: false } }
              : module_def_id.includes('lab')
                ? { Lab: { ticks_since_last_run: 0, assigned_tech: null, starved: false } }
                : { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } }
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...station,
              inventory: station.inventory.filter(
                (i) => !(i.kind === 'Module' && i.item_id === module_item_id)
              ),
              modules: [
                ...station.modules,
                {
                  id: module_id,
                  def_id: module_def_id,
                  enabled: false,
                  kind_state: kindState,
                  wear: { wear: 0 },
                },
              ],
            },
          }
        }
        break
      }

      case 'ModuleToggled': {
        const { station_id, module_id, enabled } = event as {
          station_id: string
          module_id: string
          enabled: boolean
        }
        if (updatedStations[station_id]) {
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...updatedStations[station_id],
              modules: updatedStations[station_id].modules.map((m) =>
                m.id === module_id ? { ...m, enabled } : m
              ),
            },
          }
        }
        break
      }

      case 'ModuleThresholdSet': {
        const { station_id, module_id, threshold_kg } = event as {
          station_id: string
          module_id: string
          threshold_kg: number
        }
        if (updatedStations[station_id]) {
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...updatedStations[station_id],
              modules: updatedStations[station_id].modules.map((m) => {
                if (m.id !== module_id) return m
                const ks = m.kind_state
                if (typeof ks === 'object' && 'Processor' in ks) {
                  return { ...m, kind_state: { Processor: { ...ks.Processor, threshold_kg } } }
                }
                return m
              }),
            },
          }
        }
        break
      }

      case 'RefineryRan': {
        const { station_id, ore_consumed_kg, material_produced_kg, material_quality, slag_produced_kg, material_element } = event as {
          station_id: string
          ore_consumed_kg: number
          material_produced_kg: number
          material_quality: number
          slag_produced_kg: number
          material_element: string
        }
        const REFINERY_ELEMENT = material_element as string
        if (updatedStations[station_id]) {
          let stationInv = [...updatedStations[station_id].inventory]

          // Consume ore_consumed_kg from Ore items FIFO
          let remaining = ore_consumed_kg
          stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
            if (remaining > 0 && item.kind === 'Ore') {
              const take = Math.min(item.kg, remaining)
              remaining -= take
              if (item.kg - take > 0.001) {
                acc.push({ ...item, kg: item.kg - take })
              }
              return acc
            }
            acc.push(item)
            return acc
          }, [])

          // Merge material into existing lot of same element, or push new
          if (material_produced_kg > 0.001) {
            const matIndex = stationInv.findIndex((i) => i.kind === 'Material' && i.element === REFINERY_ELEMENT)
            if (matIndex >= 0) {
              const existing = stationInv[matIndex] as MaterialItem
              const total = existing.kg + material_produced_kg
              stationInv[matIndex] = {
                ...existing,
                kg: total,
                quality: (existing.kg * existing.quality + material_produced_kg * material_quality) / total,
              }
            } else {
              stationInv.push({ kind: 'Material', element: REFINERY_ELEMENT, kg: material_produced_kg, quality: material_quality })
            }
          }

          // Blend or add slag
          if (slag_produced_kg > 0.001) {
            const existingIndex = stationInv.findIndex((i) => i.kind === 'Slag')
            if (existingIndex >= 0) {
              const existing = stationInv[existingIndex] as SlagItem
              stationInv[existingIndex] = { ...existing, kg: existing.kg + slag_produced_kg }
            } else {
              stationInv.push({ kind: 'Slag', kg: slag_produced_kg, composition: {} })
            }
          }

          updatedStations = {
            ...updatedStations,
            [station_id]: { ...updatedStations[station_id], inventory: stationInv },
          }
        }
        break
      }

      case 'AssemblerRan': {
        const { station_id, material_consumed_kg, material_element, component_produced_id, component_produced_count, component_quality } = event as {
          station_id: string
          material_consumed_kg: number
          material_element: string
          component_produced_id: string
          component_produced_count: number
          component_quality: number
        }
        if (updatedStations[station_id]) {
          let stationInv = [...updatedStations[station_id].inventory]

          // Consume material
          let remaining = material_consumed_kg
          stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
            if (remaining > 0 && item.kind === 'Material' && item.element === material_element) {
              const take = Math.min(item.kg, remaining)
              remaining -= take
              if (item.kg - take > 0.001) {
                acc.push({ ...item, kg: item.kg - take })
              }
              return acc
            }
            acc.push(item)
            return acc
          }, [])

          // Merge or create component
          const compIndex = stationInv.findIndex(
            (i) => i.kind === 'Component' && (i as ComponentItem).component_id === component_produced_id
          )
          if (compIndex >= 0) {
            const existing = stationInv[compIndex] as ComponentItem
            stationInv[compIndex] = { ...existing, count: existing.count + component_produced_count }
          } else {
            stationInv.push({
              kind: 'Component',
              component_id: component_produced_id,
              count: component_produced_count,
              quality: component_quality,
            })
          }

          updatedStations = {
            ...updatedStations,
            [station_id]: { ...updatedStations[station_id], inventory: stationInv },
          }
        }
        break
      }

      case 'WearAccumulated': {
        const { station_id, module_id, wear_after } = event as {
          station_id: string
          module_id: string
          wear_after: number
        }
        if (updatedStations[station_id]) {
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...updatedStations[station_id],
              modules: updatedStations[station_id].modules.map((m) =>
                m.id === module_id ? { ...m, wear: { wear: wear_after } } : m
              ),
            },
          }
        }
        break
      }

      case 'ModuleAutoDisabled': {
        const { station_id, module_id } = event as {
          station_id: string
          module_id: string
        }
        if (updatedStations[station_id]) {
          updatedStations = {
            ...updatedStations,
            [station_id]: {
              ...updatedStations[station_id],
              modules: updatedStations[station_id].modules.map((m) =>
                m.id === module_id ? { ...m, enabled: false } : m
              ),
            },
          }
        }
        break
      }

      case 'MaintenanceRan': {
        const { station_id, target_module_id, wear_after, repair_kits_remaining } = event as {
          station_id: string
          target_module_id: string
          wear_after: number
          repair_kits_remaining: number
        }
        if (updatedStations[station_id]) {
          const station = updatedStations[station_id]
          // Update target module's wear
          const updatedModules = station.modules.map((m) =>
            m.id === target_module_id ? { ...m, wear: { wear: wear_after } } : m
          )
          // Update RepairKit count in inventory
          const updatedInventory = station.inventory.map((item) => {
            if (item.kind === 'Component' && (item as ComponentItem).component_id === 'repair_kit') {
              return { ...item, count: repair_kits_remaining } as ComponentItem
            }
            return item
          })
          updatedStations = {
            ...updatedStations,
            [station_id]: { ...station, modules: updatedModules, inventory: updatedInventory },
          }
        }
        break
      }

      case 'ScanResult': {
        const { asteroid_id, tags } = event as { asteroid_id: string; tags: [string, number][] }
        if (updatedAsteroids[asteroid_id]) {
          updatedAsteroids = {
            ...updatedAsteroids,
            [asteroid_id]: {
              ...updatedAsteroids[asteroid_id],
              knowledge: { ...updatedAsteroids[asteroid_id].knowledge, tag_beliefs: tags },
            },
          }
        }
        break
      }

      case 'CompositionMapped': {
        const { asteroid_id, composition } = event as { asteroid_id: string; composition: Record<string, number> }
        if (updatedAsteroids[asteroid_id]) {
          updatedAsteroids = {
            ...updatedAsteroids,
            [asteroid_id]: {
              ...updatedAsteroids[asteroid_id],
              knowledge: { ...updatedAsteroids[asteroid_id].knowledge, composition },
            },
          }
        }
        break
      }

      case 'TechUnlocked': {
        const { tech_id } = event as { tech_id: string }
        updatedResearch = {
          ...updatedResearch,
          unlocked: [...updatedResearch.unlocked, tech_id],
        }
        break
      }

      case 'ScanSiteSpawned': {
        const { site_id, node, template_id } = event as { site_id: string; node: string; template_id: string }
        updatedScanSites.push({ id: site_id, node, template_id })
        break
      }
    }

    if (e['TaskStarted']) {
      const { ship_id, task_kind, target } = e['TaskStarted'] as {
        ship_id: string
        task_kind: string
        target: string | null
      }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: {
            ...updatedShips[ship_id],
            task: buildTaskStub(task_kind, target, evt.tick),
          },
        }
      }
    }

    if (e['TaskCompleted']) {
      const { ship_id } = e['TaskCompleted'] as { ship_id: string }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: { ...updatedShips[ship_id], task: null },
        }
      }
    }

    if (e['ShipArrived']) {
      const { ship_id, node } = e['ShipArrived'] as { ship_id: string; node: string }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: { ...updatedShips[ship_id], location_node: node },
        }
      }
    }

    if (e['DataGenerated']) {
      const { kind, amount } = e['DataGenerated'] as { kind: string; amount: number }
      updatedResearch = {
        ...updatedResearch,
        data_pool: {
          ...updatedResearch.data_pool,
          [kind]: (updatedResearch.data_pool[kind] ?? 0) + amount,
        },
      }
    }
  }

  return {
    asteroids: updatedAsteroids,
    ships: updatedShips,
    stations: updatedStations,
    research: updatedResearch,
    scanSites: updatedScanSites,
  }
}
