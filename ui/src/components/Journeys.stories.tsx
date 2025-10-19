import type { Meta, StoryObj } from '@storybook/react';
import { Journeys } from './Journeys';
import { within, userEvent } from '@storybook/testing-library';
import { expect } from '@storybook/jest';

// Mock data
const mockJourney: JourneyResponse = {
  journey_type: 'adapter-lifecycle',
  id: 'test-id',
  data: { total_states: 5 },
  states: [
    { state: 'unloaded', timestamp: new Date().toISOString(), details: { memory: 0 } },
    // ... more
  ],
  created_at: new Date().toISOString(),
};

const mock50States = { ...mockJourney, states: Array(50).fill(mockJourney.states[0]) };

const meta: Meta<typeof Journeys> = {
  title: 'Components/Journeys',
  component: Journeys,
  parameters: {
    layout: 'centered',
  },
  tags: ['autodocs'],
  argTypes: {
    user: { control: 'object' },
    selectedTenant: { control: 'text' },
  },
  decorators: [
    (Story) => (
      <div className="p-8 bg-background min-h-screen">
        <Story />
      </div>
    ),
  ],
};

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    user: { email: 'user@example.com', roles: ['user'] },
    selectedTenant: 'default',
  },
};

export const WithData: Story = {
  args: { ...Default.args },
  play: async ({ canvasElement }) => {
    // Mock Query data via MSW or assume
    await expect(canvasElement).toBeInTheDocument();
  },
};

export const ErrorState: Story = {
  args: { ...Default.args },
  parameters: { nextjs: { appDirectory: true } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // Simulate error
    await userEvent.type(canvas.getByLabelText('Enter journey ID'), 'invalid');
    // Assert toast or error div
  },
};

export const DarkMode: Story = {
  args: { ...Default.args },
  parameters: {
    backgrounds: { default: 'dark' },
  },
};
