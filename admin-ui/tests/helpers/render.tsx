import { render } from '@solidjs/testing-library';
import { Router, Route } from '@solidjs/router';
import { QueryClientProvider, QueryClient } from '@tanstack/solid-query';
import type { JSX } from 'solid-js';

export function renderWithProviders(ui: () => JSX.Element) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });

  return render(() => (
    <QueryClientProvider client={queryClient}>
      <Router>
        <Route path="*" component={() => ui()} />
      </Router>
    </QueryClientProvider>
  ));
}
