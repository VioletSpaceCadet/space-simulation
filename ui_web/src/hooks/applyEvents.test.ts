import { describe, expect, it } from 'vitest'
import { applyEvents } from './applyEvents'
import type { AsteroidState, OreItem, ResearchState, ShipState, StationState } from '../types'

const emptyResearch: ResearchState = { unlocked: [], data_pool: {}, evidence: {} }

function makeShip(overrides: Partial<ShipState> = {}): ShipState {
  return {
    id: 'ship_0001',
    location_node: 'node_a',
    owner: 'player',
    inventory: [],
    cargo_capacity_m3: 20,
    task: null,
    ...overrides,
  }
}

function makeAsteroid(overrides: Partial<AsteroidState> = {}): AsteroidState {
  return {
    id: 'ast_001',
    location_node: 'node_a',
    anomaly_tags: [],
    mass_kg: 500,
    knowledge: { tag_beliefs: [], composition: null },
    ...overrides,
  }
}

function makeStation(overrides: Partial<StationState> = {}): StationState {
  return {
    id: 'station_001',
    location_node: 'node_a',
    power_available_per_tick: 10,
    inventory: [],
    cargo_capacity_m3: 100,
    facilities: { compute_units_total: 1, power_per_compute_unit_per_tick: 1, efficiency: 1 },
    modules: [],
    ...overrides,
  }
}

function makeOreLot(overrides: Partial<OreItem> = {}): OreItem {
  return {
    kind: 'Ore',
    lot_id: 'lot_1',
    asteroid_id: 'ast_001',
    kg: 50,
    composition: { Fe: 0.7, Si: 0.3 },
    ...overrides,
  }
}

describe('applyEvents', () => {
  describe('OreMined', () => {
    it('updates ship inventory and asteroid mass', () => {
      const ship = makeShip()
      const asteroid = makeAsteroid({ mass_kg: 500 })
      const oreLot = makeOreLot()

      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          OreMined: {
            ship_id: 'ship_0001',
            asteroid_id: 'ast_001',
            ore_lot: oreLot,
            asteroid_remaining_kg: 450,
          },
        },
      }]

      const result = applyEvents(
        { ast_001: asteroid },
        { ship_0001: ship },
        {},
        emptyResearch,
        [],
        events,
      )

      expect(result.ships['ship_0001'].inventory).toHaveLength(1)
      expect(result.ships['ship_0001'].inventory[0]).toEqual(oreLot)
      expect(result.asteroids['ast_001'].mass_kg).toBe(450)
    })

    it('removes depleted asteroid when asteroid_remaining_kg is zero', () => {
      const ship = makeShip()
      const asteroid = makeAsteroid({ mass_kg: 10 })
      const oreLot = makeOreLot({ kg: 10 })

      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          OreMined: {
            ship_id: 'ship_0001',
            asteroid_id: 'ast_001',
            ore_lot: oreLot,
            asteroid_remaining_kg: 0,
          },
        },
      }]

      const result = applyEvents(
        { ast_001: asteroid },
        { ship_0001: ship },
        {},
        emptyResearch,
        [],
        events,
      )

      expect(result.asteroids['ast_001']).toBeUndefined()
      // Ship still gets the ore even though asteroid is depleted
      expect(result.ships['ship_0001'].inventory).toHaveLength(1)
      expect(result.ships['ship_0001'].inventory[0]).toEqual(oreLot)
    })
  })

  describe('OreDeposited', () => {
    it('clears ship inventory and adds items to station', () => {
      const oreLot = makeOreLot()
      const ship = makeShip({ inventory: [oreLot] })
      const station = makeStation()

      const depositedItems = [makeOreLot({ lot_id: 'lot_1', kg: 50 })]

      const events = [{
        id: 'e1',
        tick: 20,
        event: {
          OreDeposited: {
            ship_id: 'ship_0001',
            station_id: 'station_001',
            items: depositedItems,
          },
        },
      }]

      const result = applyEvents(
        {},
        { ship_0001: ship },
        { station_001: station },
        emptyResearch,
        [],
        events,
      )

      expect(result.ships['ship_0001'].inventory).toEqual([])
      expect(result.stations['station_001'].inventory).toHaveLength(1)
      expect(result.stations['station_001'].inventory[0]).toEqual(depositedItems[0])
    })

    it('appends to existing station inventory', () => {
      const existingOre = makeOreLot({ lot_id: 'lot_existing', kg: 100 })
      const station = makeStation({ inventory: [existingOre] })
      const ship = makeShip({ inventory: [makeOreLot()] })

      const newOre = makeOreLot({ lot_id: 'lot_new', kg: 75 })

      const events = [{
        id: 'e1',
        tick: 20,
        event: {
          OreDeposited: {
            ship_id: 'ship_0001',
            station_id: 'station_001',
            items: [newOre],
          },
        },
      }]

      const result = applyEvents(
        {},
        { ship_0001: ship },
        { station_001: station },
        emptyResearch,
        [],
        events,
      )

      expect(result.stations['station_001'].inventory).toHaveLength(2)
      expect(result.stations['station_001'].inventory[0]).toEqual(existingOre)
      expect(result.stations['station_001'].inventory[1]).toEqual(newOre)
      expect(result.ships['ship_0001'].inventory).toEqual([])
    })
  })
})
