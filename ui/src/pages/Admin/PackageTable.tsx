import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { AdapterPackage } from '@/api/types';

interface PackageTableProps {
  packages: AdapterPackage[];
  onEdit?: (pkg: AdapterPackage) => void;
  onDelete?: (pkg: AdapterPackage) => void;
  onInstall?: (pkg: AdapterPackage) => void;
  onUninstall?: (pkg: AdapterPackage) => void;
}

export function PackageTable({ packages, onEdit, onDelete, onInstall, onUninstall }: PackageTableProps) {
  if (!packages.length) {
    return <div className="text-sm text-muted-foreground">No packages yet.</div>;
  }

  return (
    <div className="overflow-x-auto">
      <table className="min-w-full text-left text-sm">
        <thead className="border-b">
          <tr className="[&>th]:px-3 [&>th]:py-2 text-muted-foreground">
            <th>Name</th>
            <th>Stack</th>
            <th>Adapters</th>
            <th>Determinism</th>
            <th>Domain</th>
            <th>Installed</th>
            <th>Tags</th>
            <th className="text-right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {packages.map((pkg) => (
            <tr key={pkg.id} className="border-b last:border-0 align-top">
              <td className="[&>div]:py-2">
                <div className="font-medium">{pkg.name}</div>
                <div className="text-xs text-muted-foreground truncate max-w-xs">
                  {pkg.description || '—'}
                </div>
              </td>
              <td className="px-3 py-2 text-xs text-muted-foreground">
                {pkg.stack_id}
              </td>
              <td className="px-3 py-2 text-xs">
                {pkg.adapter_ids?.length ?? pkg.adapter_strengths?.length ?? 0}
              </td>
              <td className="px-3 py-2 text-xs">
                {pkg.determinism_mode || 'inherit'}
                {pkg.routing_determinism_mode ? ` / ${pkg.routing_determinism_mode}` : ''}
              </td>
              <td className="px-3 py-2 text-xs">
                {pkg.domain ? <Badge variant="outline">{pkg.domain}</Badge> : '—'}
              </td>
              <td className="px-3 py-2 text-xs space-x-2">
                {pkg.installed ? (
                  <Badge variant="default">Installed</Badge>
                ) : (
                  <Badge variant="secondary">Not installed</Badge>
                )}
                {pkg.scope_path_prefix && (
                  <Badge variant="outline" className="text-xs">
                    {pkg.scope_path_prefix}
                  </Badge>
                )}
              </td>
              <td className="px-3 py-2 space-x-1">
                {(pkg.tags || []).map((tag) => (
                  <Badge key={tag} variant="secondary" className="text-xs">
                    {tag}
                  </Badge>
                ))}
              </td>
              <td className="px-3 py-2 text-right space-x-2">
                {pkg.installed
                  ? onUninstall && (
                      <Button size="sm" variant="outline" onClick={() => onUninstall(pkg)}>
                        Uninstall
                      </Button>
                    )
                  : onInstall && (
                      <Button size="sm" variant="outline" onClick={() => onInstall(pkg)}>
                        Install
                      </Button>
                    )}
                {onEdit && (
                  <Button size="sm" variant="outline" onClick={() => onEdit(pkg)}>
                    Edit
                  </Button>
                )}
                {onDelete && (
                  <Button size="sm" variant="destructive" onClick={() => onDelete(pkg)}>
                    Delete
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default PackageTable;

