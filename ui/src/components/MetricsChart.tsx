import React, { useRef, useEffect } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

export interface MetricData {
  time: string;
  value: number;
  label?: string;
}

interface MetricsChartProps {
  data: MetricData[];
  title?: string;
  yAxisLabel?: string;
  color?: string;
  height?: number;
}

export const MetricsChart: React.FC<MetricsChartProps> = ({
  data,
  title,
  yAxisLabel = 'Value',
  color = '#8884d8',
  height = 300,
}) => {
  return (
    <div className="w-full">
      {title && (
        <h3 className="text-lg font-semibold mb-4">{title}</h3>
      )}
      <ResponsiveContainer width="100%" height={height}>
        <LineChart data={data} margin={{ top: 5, right: 30, left: 20, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis 
            dataKey="time" 
            tick={{ fontSize: 12 }}
            tickFormatter={(value) => new Date(value).toLocaleTimeString()}
          />
          <YAxis 
            label={{ value: yAxisLabel, angle: -90, position: 'insideLeft' }}
            tick={{ fontSize: 12 }}
          />
          <Tooltip 
            labelFormatter={(value) => new Date(value).toLocaleString()}
            formatter={(value: number) => [value.toFixed(2), yAxisLabel]}
          />
          <Line 
            type="monotone" 
            dataKey="value" 
            stroke={color} 
            strokeWidth={2}
            dot={{ r: 4 }}
            activeDot={{ r: 6 }}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
};

// Alternative implementation using Canvas API for better performance
export const CanvasChart: React.FC<MetricsChartProps> = ({
  data,
  title,
  yAxisLabel = 'Value',
  color = '#8884d8',
  height = 300,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || data.length === 0) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Set canvas size
    canvas.width = canvas.offsetWidth * window.devicePixelRatio;
    canvas.height = height * window.devicePixelRatio;
    ctx.scale(window.devicePixelRatio, window.devicePixelRatio);

    // Clear canvas
    ctx.clearRect(0, 0, canvas.offsetWidth, height);

    // Calculate dimensions
    const padding = 40;
    const chartWidth = canvas.offsetWidth - 2 * padding;
    const chartHeight = height - 2 * padding;

    // Find min/max values
    const values = data.map(d => d.value);
    const minValue = Math.min(...values);
    const maxValue = Math.max(...values);
    const valueRange = maxValue - minValue || 1;

    // Draw axes
    ctx.strokeStyle = '#e5e7eb';
    ctx.lineWidth = 1;
    
    // Y-axis
    ctx.beginPath();
    ctx.moveTo(padding, padding);
    ctx.lineTo(padding, height - padding);
    ctx.stroke();

    // X-axis
    ctx.beginPath();
    ctx.moveTo(padding, height - padding);
    ctx.lineTo(canvas.offsetWidth - padding, height - padding);
    ctx.stroke();

    // Draw grid lines
    ctx.strokeStyle = '#f3f4f6';
    ctx.lineWidth = 0.5;
    for (let i = 0; i <= 5; i++) {
      const y = padding + (chartHeight * i) / 5;
      ctx.beginPath();
      ctx.moveTo(padding, y);
      ctx.lineTo(canvas.offsetWidth - padding, y);
      ctx.stroke();
    }

    // Draw line
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();

    data.forEach((point, index) => {
      const x = padding + (chartWidth * index) / (data.length - 1);
      const y = height - padding - (chartHeight * (point.value - minValue)) / valueRange;
      
      if (index === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    });

    ctx.stroke();

    // Draw points
    ctx.fillStyle = color;
    data.forEach((point, index) => {
      const x = padding + (chartWidth * index) / (data.length - 1);
      const y = height - padding - (chartHeight * (point.value - minValue)) / valueRange;
      
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, 2 * Math.PI);
      ctx.fill();
    });

    // Draw labels
    ctx.fillStyle = '#6b7280';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';
    
    // Y-axis labels
    for (let i = 0; i <= 5; i++) {
      const value = minValue + (valueRange * i) / 5;
      const y = height - padding - (chartHeight * i) / 5;
      ctx.fillText(value.toFixed(1), padding - 10, y + 4);
    }

    // X-axis labels
    ctx.textAlign = 'center';
    for (let i = 0; i < data.length; i += Math.max(1, Math.floor(data.length / 5))) {
      const x = padding + (chartWidth * i) / (data.length - 1);
      const time = new Date(data[i].time).toLocaleTimeString();
      ctx.fillText(time, x, height - padding + 20);
    }

  }, [data, color, height]);

  return (
    <div className="w-full">
      {title && (
        <h3 className="text-lg font-semibold mb-4">{title}</h3>
      )}
      <canvas
        ref={canvasRef}
        className="w-full border rounded"
        style={{ height: `${height}px` }}
      />
    </div>
  );
};
