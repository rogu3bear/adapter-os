import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { toast } from 'sonner';

interface RegisterAdapterFormProps {
  onClose: () => void;
}

export function RegisterAdapterForm({ onClose }: RegisterAdapterFormProps) {
  const [formData, setFormData] = useState({
    name: '',
    adapter_hash: '',
    capability_tags: '',
    tier: 'persistent',
    rank: 16,
    framework: '',
    framework_version: '',
  });

  return (
    <SectionErrorBoundary sectionName="Register Form">
      <div className="space-y-4">
        <div>
          <Label htmlFor="name">Adapter Name</Label>
          <Input
            id="name"
            value={formData.name}
            onChange={(e) => setFormData((prev) => ({ ...prev, name: e.target.value }))}
            placeholder="my-adapter-v1"
          />
        </div>
        <div>
          <Label htmlFor="adapter_hash">Adapter Hash</Label>
          <Input
            id="adapter_hash"
            value={formData.adapter_hash}
            onChange={(e) => setFormData((prev) => ({ ...prev, adapter_hash: e.target.value }))}
            placeholder="b3:abc123..."
          />
        </div>
        <div>
          <Label htmlFor="capability_tags">Capability Tags</Label>
          <Input
            id="capability_tags"
            value={formData.capability_tags}
            onChange={(e) => setFormData((prev) => ({ ...prev, capability_tags: e.target.value }))}
            placeholder="python,django,web"
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <Label htmlFor="tier">Tier</Label>
            <Select value={formData.tier} onValueChange={(value) => setFormData((prev) => ({ ...prev, tier: value }))}>
              <SelectTrigger>
                <SelectValue placeholder="Select tier" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="persistent">Persistent</SelectItem>
                <SelectItem value="ephemeral">Ephemeral</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div>
            <Label htmlFor="rank">Rank</Label>
            <Input
              id="rank"
              type="number"
              value={formData.rank}
              onChange={(e) => setFormData((prev) => ({ ...prev, rank: parseInt(e.target.value) || 16 }))}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <Label htmlFor="framework">Framework</Label>
            <Input
              id="framework"
              value={formData.framework}
              onChange={(e) => setFormData((prev) => ({ ...prev, framework: e.target.value }))}
              placeholder="django"
            />
          </div>
          <div>
            <Label htmlFor="framework_version">Framework Version</Label>
            <Input
              id="framework_version"
              value={formData.framework_version}
              onChange={(e) => setFormData((prev) => ({ ...prev, framework_version: e.target.value }))}
              placeholder="4.2"
            />
          </div>
        </div>
        <div className="flex justify-end space-x-2">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={() => {
            toast.info('Adapter registration coming soon');
            onClose();
          }}>
            Register Adapter
          </Button>
        </div>
      </div>
    </SectionErrorBoundary>
  );
}
