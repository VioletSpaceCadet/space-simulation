import { describe, expect, it } from 'vitest';

import type { ResearchState, TechDef } from '../types';

import { computeTreeState } from './techTree';

function makeTech(id: string, prereqs: string[] = [], domain_requirements: Record<string, number> = {}): TechDef {
  return {
    id,
    name: `Tech ${id}`,
    prereqs,
    domain_requirements,
    accepted_data: [],
    difficulty: 100,
    effects: [],
  };
}

function makeResearch(unlocked: string[] = [], evidence: ResearchState['evidence'] = {}): ResearchState {
  return { unlocked, data_pool: {}, evidence, action_counts: {} };
}

describe('computeTreeState', () => {
  it('unlocked tech is visible with unlocked state', () => {
    const techs = [makeTech('tech_a')];
    const research = makeResearch(['tech_a']);
    const { nodes } = computeTreeState(techs, research, []);
    expect(nodes.get('tech_a')?.state).toBe('unlocked');
  });

  it('tech with lab assigned and prereqs met is researching', () => {
    const techs = [makeTech('tech_a'), makeTech('tech_b', ['tech_a'])];
    const research = makeResearch(['tech_a']);
    const { nodes } = computeTreeState(techs, research, ['tech_b']);
    expect(nodes.get('tech_b')?.state).toBe('researching');
  });

  it('direct child of researching tech is locked', () => {
    const techs = [makeTech('tech_a'), makeTech('tech_b', ['tech_a']), makeTech('tech_c', ['tech_b'])];
    const research = makeResearch(['tech_a']);
    const { nodes } = computeTreeState(techs, research, ['tech_b']);
    expect(nodes.get('tech_c')?.state).toBe('locked');
  });

  it('grandchild of researching tech is mystery', () => {
    const techs = [
      makeTech('tech_a'),
      makeTech('tech_b', ['tech_a']),
      makeTech('tech_c', ['tech_b']),
      makeTech('tech_d', ['tech_c']),
    ];
    const research = makeResearch(['tech_a']);
    const { nodes } = computeTreeState(techs, research, ['tech_b']);
    expect(nodes.get('tech_d')?.state).toBe('mystery');
  });

  it('empty state returns no visible nodes', () => {
    const techs = [makeTech('tech_a'), makeTech('tech_b', ['tech_a'])];
    const research = makeResearch([]);
    const { nodes } = computeTreeState(techs, research, []);
    expect(nodes.size).toBe(0);
  });

  it('edge from researching to locked is dim', () => {
    const techs = [makeTech('tech_a'), makeTech('tech_b', ['tech_a']), makeTech('tech_c', ['tech_b'])];
    const research = makeResearch(['tech_a']);
    const { edges } = computeTreeState(techs, research, ['tech_b']);
    const dimEdge = edges.find(edge => edge.from === 'tech_b' && edge.to === 'tech_c');
    expect(dimEdge?.style).toBe('dim');
  });

  it('locked child with mixed visible/invisible prereqs shows edges only from visible parents', () => {
    // tech_c requires both tech_b (researching/visible) and tech_x (invisible, no parent)
    const techs = [
      makeTech('tech_a'),
      makeTech('tech_b', ['tech_a']),
      makeTech('tech_c', ['tech_b', 'tech_x']),
    ];
    const research = makeResearch(['tech_a']);
    const { edges } = computeTreeState(techs, research, ['tech_b']);
    // Edge from tech_b (researching) to tech_c (locked) should exist
    const edgeFromB = edges.find(edge => edge.from === 'tech_b' && edge.to === 'tech_c');
    expect(edgeFromB).toBeDefined();
    // No edge from tech_x since it's not visible
    const edgeFromX = edges.find(edge => edge.from === 'tech_x' && edge.to === 'tech_c');
    expect(edgeFromX).toBeUndefined();
  });

  it('converging edges into mystery zone: two locked parents to mystery child', () => {
    // tech_a (unlocked) -> tech_b (researching) -> tech_c (locked)
    // tech_a (unlocked) -> tech_d (locked)
    // tech_e has prereqs tech_c and tech_d -> mystery
    const techs = [
      makeTech('tech_a'),
      makeTech('tech_b', ['tech_a']),
      makeTech('tech_c', ['tech_b']),
      makeTech('tech_d', ['tech_a']),
      makeTech('tech_e', ['tech_c', 'tech_d']),
    ];
    const research = makeResearch(['tech_a']);
    const { nodes, edges } = computeTreeState(techs, research, ['tech_b']);

    expect(nodes.get('tech_e')?.state).toBe('mystery');
    // Both edges to tech_e should be fade
    const edgesIntoE = edges.filter(edge => edge.to === 'tech_e');
    expect(edgesIntoE).toHaveLength(2);
    expect(edgesIntoE.every(edge => edge.style === 'fade')).toBe(true);
  });
});
