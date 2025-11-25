import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { TERMS } from '@/constants/terminology';

interface Collection {
  id: string;
  name: string;
  document_count: number;
}

interface Props {
  collections: Collection[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  placeholder?: string;
}

export function CollectionSelector({ collections, selectedId, onSelect, placeholder }: Props) {
  return (
    <Select value={selectedId || undefined} onValueChange={onSelect}>
      <SelectTrigger>
        <SelectValue placeholder={placeholder || TERMS.selectDataset} />
      </SelectTrigger>
      <SelectContent>
        {collections.length === 0 ? (
          <SelectItem value="" disabled>
            No collections available
          </SelectItem>
        ) : (
          collections.map(c => (
            <SelectItem key={c.id} value={c.id}>
              {c.name} ({c.document_count} docs)
            </SelectItem>
          ))
        )}
      </SelectContent>
    </Select>
  );
}
