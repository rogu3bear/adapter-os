import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Waves } from 'lucide-react';
import { useDemoMode } from '@/hooks/demo/DemoProvider';

export function ScenarioController() {
  const { enabled, simulateTraffic, setSimulateTraffic, activeModel } = useDemoMode();

  if (!enabled) return null;

  return (
    <Card className="mb-4 border-dashed bg-muted/40">
      <CardContent className="flex flex-col gap-3 py-4">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Badge variant="secondary" className="uppercase tracking-wide text-[10px]">
              Demo Controls
            </Badge>
            <span className="text-sm text-muted-foreground">Simulate a high-load adapter showcase</span>
          </div>
          <Badge variant="outline" className="text-xs">
            Active Model: {activeModel.name}
          </Badge>
        </div>
        <div className="flex items-center gap-3">
          <Switch
            id="simulate-traffic"
            checked={simulateTraffic}
            onCheckedChange={setSimulateTraffic}
          />
          <Label htmlFor="simulate-traffic" className="cursor-pointer flex items-center gap-2 text-sm">
            <Waves className="h-4 w-4 text-primary" />
            Simulate Traffic
          </Label>
          <span className="text-xs text-muted-foreground">
            Drives sine-wave CPU + request charts
          </span>
        </div>
      </CardContent>
    </Card>
  );
}

export default ScenarioController;
