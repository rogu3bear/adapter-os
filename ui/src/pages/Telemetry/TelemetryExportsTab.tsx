import { useMutation, useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import type { TelemetryBundle } from '@/api/types';
import { useToast } from '@/hooks/use-toast';

export default function TelemetryExportsTab() {
  const { toast } = useToast();
  const { data: bundles, refetch, isLoading } = useQuery({
    queryKey: ['telemetry-bundles'],
    queryFn: () => apiClient.listTelemetryBundles(),
  });

  const generate = useMutation({
    mutationFn: () => apiClient.generateTelemetryBundle(),
    onSuccess: () => {
      toast({ title: 'Export bundle requested' });
      refetch();
    },
  });

  const exportBundle = useMutation({
    mutationFn: (bundleId: string) => apiClient.exportTelemetryBundle(bundleId),
    onSuccess: (res) => {
      toast({ title: 'Export ready', description: `Bundle size ${res.size_bytes} bytes` });
    },
  });

  if (isLoading) {
    return <div className="text-sm text-muted-foreground">Loading exports…</div>;
  }

  return (
    <Card>
      <CardHeader className="flex items-center justify-between">
        <CardTitle>Telemetry exports</CardTitle>
        <Button onClick={() => generate.mutate()} disabled={generate.isPending}>
          {generate.isPending ? 'Generating…' : 'Generate bundle'}
        </Button>
      </CardHeader>
      <CardContent>
        {!bundles?.length ? (
          <div className="text-sm text-muted-foreground">No telemetry bundles found.</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>CPID</TableHead>
                <TableHead>Events</TableHead>
                <TableHead>Size</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {bundles.map((bundle: TelemetryBundle) => (
                <TableRow key={bundle.id}>
                  <TableCell className="font-mono text-xs">{bundle.id}</TableCell>
                  <TableCell>{bundle.cpid}</TableCell>
                  <TableCell>{bundle.event_count}</TableCell>
                  <TableCell>{bundle.size_bytes} bytes</TableCell>
                  <TableCell className="text-right">
                    <Button size="sm" variant="outline" onClick={() => exportBundle.mutate(bundle.id)}>
                      Download
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
