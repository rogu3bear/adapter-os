import React from 'react';
import { Card, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { CheckCircle, AlertTriangle } from 'lucide-react';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';
import { CATEGORY_ICONS, CATEGORY_DESCRIPTIONS } from '@/components/TrainingWizard/constants';

export function CategoryStep() {
  const { state, updateState } = useTrainingWizardContext();
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Select the type of adapter you want to train. Each category has specific configuration options.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {(['code', 'framework', 'codebase', 'ephemeral'] as const).map((cat) => {
          const Icon = CATEGORY_ICONS[cat];
          const isSelected = state.category === cat;
          return (
            <Card
              key={cat}
              className={`cursor-pointer transition-all hover:border-primary ${
                isSelected ? 'border-primary bg-primary/5' : ''
              }`}
              onClick={() => updateState({ category: cat })}
            >
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Icon className="h-5 w-5" />
                  <span className="capitalize">{cat} Adapter</span>
                  {isSelected && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
                </CardTitle>
                <CardDescription>{CATEGORY_DESCRIPTIONS[cat]}</CardDescription>
              </CardHeader>
            </Card>
          );
        })}
      </div>
      {!state.category && (
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>Please select an adapter category to continue</AlertDescription>
        </Alert>
      )}
    </div>
  );
}
