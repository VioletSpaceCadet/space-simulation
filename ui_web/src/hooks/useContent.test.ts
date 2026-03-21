import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import * as api from '../api';
import type { ContentResponse } from '../types';

import { useContent } from './useContent';

vi.mock('../api');

const mockContent: ContentResponse = {
  techs: [{ id: 'tech_a', name: 'Tech A', prereqs: [], domain_requirements: {}, accepted_data: [], difficulty: 100, effects: [] }],
  lab_rates: [],
  data_rates: {},
  minutes_per_tick: 60,
};

describe('useContent', () => {
  it('fetches content on mount', async () => {
    vi.mocked(api.fetchContent).mockResolvedValue(mockContent);
    const { result } = renderHook(() => useContent());
    await waitFor(() => expect(result.current.content).not.toBeNull());
    expect(result.current.content?.techs).toHaveLength(1);
  });

  it('returns null before load', () => {
    vi.mocked(api.fetchContent).mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useContent());
    expect(result.current.content).toBeNull();
  });
});
