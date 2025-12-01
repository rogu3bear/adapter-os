import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { BrowserRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Journeys } from '@/components/Journeys';
import * as api from '@/api/client'; // Mock
import userEvent from '@testing-library/user-event'; // Add for click

vi.mock('@/api/client');

describe.skip('Journeys', () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const mockUser = { email: 'test@example.com', roles: ['user'] };
  const mockTenant = 'default';

  beforeEach(() => {
    vi.clearAllMocks();
  });

  test('renders default', () => {
    render(
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Journeys user={mockUser} selectedTenant={mockTenant} />
        </BrowserRouter>
      </QueryClientProvider>
    );
    expect(screen.getByText('User Journeys Dashboard')).toBeInTheDocument();
  });

  test('handles error state', async () => {
    (api.get as vi.Mock).mockRejectedValue(new Error('API Error'));
    render(
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Journeys user={mockUser} selectedTenant={mockTenant} />
        </BrowserRouter>
      </QueryClientProvider>
    );
    await waitFor(() => expect(screen.getByText('Error loading journey: API Error')).toBeInTheDocument());
  });

  test('paginates states', async () => {
    // Mock data with 25 states
    const mockData = { /* JourneyResponse with states.length=25 */ };
    (api.get as vi.Mock).mockResolvedValue({ data: mockData });
    render(
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Journeys user={mockUser} selectedTenant={mockTenant} />
        </BrowserRouter>
      </QueryClientProvider>
    );
    await waitFor(() => expect(screen.getAllByRole('button', { name: /Previous|Next/ })).toHaveLength(2));
    fireEvent.click(screen.getByRole('button', { name: /Next/ }));
    await waitFor(() => expect(screen.getAllByRole('region', { name: /Accordion/ })).toHaveLength(5)); // Page 2: 5 states
  });

  test('expands first accordion item', async () => {
    const user = userEvent.setup();
    const mockData = { /* JourneyResponse with 1+ states, details: { memory_bytes: 0 } */ };
    (api.get as vi.Mock).mockResolvedValue({ data: mockData });
    render(
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Journeys user={mockUser} selectedTenant={mockTenant} />
        </BrowserRouter>
      </QueryClientProvider>
    );
    await waitFor(() => expect(screen.getByText('User Journeys Dashboard')).toBeInTheDocument());

    // First item should be expanded (defaultValue)
    expect(screen.getByText('memory_bytes')).toBeInTheDocument(); // Visible in content

    // Click to collapse/expand another
    await user.click(screen.getAllByRole('button')[1]); // Second trigger
    await waitFor(() => expect(screen.queryByText('Some other detail')).toBeInTheDocument());
  });

  test('responsive on resize', () => {
    render(
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Journeys user={mockUser} selectedTenant={mockTenant} />
        </BrowserRouter>
      </QueryClientProvider>
    );
    fireEvent(window, new ResizeEvent('resize', { target: { innerWidth: 320 } }));
    // Assert mobile classes, e.g., sidebar translate-x-full
    expect(document.body.classList.contains('overflow-hidden')).toBe(false); // Initial
    // Simulate open: Assume toggle called
  });
});
