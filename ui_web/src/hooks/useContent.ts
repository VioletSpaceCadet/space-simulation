import { useCallback, useEffect, useState } from 'react';

import { fetchContent } from '../api';
import type { ContentResponse } from '../types';

const REFETCH_INTERVAL_MS = 30_000;

export function useContent() {
  const [content, setContent] = useState<ContentResponse | null>(null);

  const refetch = useCallback(() => {
    fetchContent().then(setContent).catch(console.error);
  }, []);

  useEffect(() => {
    refetch();
    const interval = setInterval(refetch, REFETCH_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [refetch]);

  return { content, refetch };
}
