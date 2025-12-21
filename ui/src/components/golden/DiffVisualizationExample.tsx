import { useState } from 'react';
import { DiffVisualization } from './DiffVisualization';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

/**
 * Example component demonstrating DiffVisualization usage
 *
 * This shows how to use the DiffVisualization component with custom texts
 */
export function DiffVisualizationExample() {
  const [goldenText, setGoldenText] = useState(
    `function calculateTotal(items) {
  let total = 0;
  for (const item of items) {
    total += item.price;
  }
  return total;
}

const result = calculateTotal([
  { name: 'Apple', price: 1.50 },
  { name: 'Banana', price: 0.75 },
]);

console.log('Total:', result);`
  );

  const [currentText, setCurrentText] = useState(
    `function calculateTotal(items, discount = 0) {
  let total = 0;
  for (const item of items) {
    total += item.price * item.quantity;
  }

  // Apply discount
  if (discount > 0) {
    total = total * (1 - discount);
  }

  return Math.round(total * 100) / 100;
}

const result = calculateTotal([
  { name: 'Apple', price: 1.50, quantity: 2 },
  { name: 'Banana', price: 0.75, quantity: 3 },
  { name: 'Orange', price: 2.00, quantity: 1 },
], 0.1);

console.log('Total with 10% discount:', result);`
  );

  const loadExample = (exampleName: string) => {
    switch (exampleName) {
      case 'code':
        // Already loaded
        break;
      case 'text':
        setGoldenText(`The quick brown fox jumps over the lazy dog.
This is a test sentence.
Another line of text here.`);
        setCurrentText(`The quick brown fox leaps over the lazy dog.
This is a modified sentence.
Another line of text here.
And a new line added.`);
        break;
      case 'inference':
        setGoldenText(`Based on the analysis of the data, I can conclude that the pattern shows a clear upward trend. The correlation coefficient of 0.87 indicates a strong positive relationship.`);
        setCurrentText(`Based on the analysis of the dataset, I can conclude that the observed pattern shows a clear upward trajectory. The correlation coefficient of 0.89 indicates a strong positive relationship between the variables.`);
        break;
      case 'large':
        // Generate large text for performance testing
        const lines: string[] = [];
        for (let i = 0; i < 500; i++) {
          lines.push(`Line ${i}: This is a sample line of text for performance testing.`);
        }
        setGoldenText(lines.join('\n'));

        const modifiedLines = [...lines];
        for (let i = 0; i < 50; i++) {
          const idx = Math.floor(Math.random() * modifiedLines.length);
          modifiedLines[idx] = `Line ${idx}: MODIFIED - This line has been changed for testing.`;
        }
        setCurrentText(modifiedLines.join('\n'));
        break;
    }
  };

  return (
    <div className="space-y-4 p-4">
      <Card className="p-4">
        <h2 className="text-lg font-semibold mb-4">Diff Visualization Example</h2>

        <div className="flex gap-2 mb-4">
          <Button variant="outline" size="sm" onClick={() => loadExample('code')}>
            Code Example
          </Button>
          <Button variant="outline" size="sm" onClick={() => loadExample('text')}>
            Text Example
          </Button>
          <Button variant="outline" size="sm" onClick={() => loadExample('inference')}>
            Inference Example
          </Button>
          <Button variant="outline" size="sm" onClick={() => loadExample('large')}>
            Large Text (Performance Test)
          </Button>
        </div>

        <div className="grid grid-cols-2 gap-4 mb-4">
          <div>
            <label className="text-sm font-medium mb-2 block">Golden Text</label>
            <Textarea
              value={goldenText}
              onChange={(e) => setGoldenText(e.target.value)}
              className="font-mono text-xs min-h-[200px]"
              placeholder="Enter golden text..."
            />
          </div>
          <div>
            <label className="text-sm font-medium mb-2 block">Current Text</label>
            <Textarea
              value={currentText}
              onChange={(e) => setCurrentText(e.target.value)}
              className="font-mono text-xs min-h-[200px]"
              placeholder="Enter current text..."
            />
          </div>
        </div>
      </Card>

      <DiffVisualization goldenText={goldenText} currentText={currentText} contextLines={3} enableVirtualization={true} />
    </div>
  );
}

export default DiffVisualizationExample;
