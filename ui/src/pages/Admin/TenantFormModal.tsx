import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useCreateTenant, useUpdateTenant } from '@/hooks/useAdmin';
import type { Tenant, CreateTenantRequest } from '@/api/types';

interface TenantFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant?: Tenant;
}

interface FormData {
  name: string;
  uid?: number;
  gid?: number;
  isolation_level?: string;
}

export function TenantFormModal({ open, onOpenChange, tenant }: TenantFormModalProps) {
  const isEdit = !!tenant;
  const createTenant = useCreateTenant();
  const updateTenant = useUpdateTenant();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
    setValue,
    watch,
  } = useForm<FormData>({
    defaultValues: {
      name: tenant?.name || '',
      uid: tenant?.uid,
      gid: tenant?.gid,
      isolation_level: tenant?.isolation_level || 'standard',
    },
  });

  const isolationLevel = watch('isolation_level');

  useEffect(() => {
    if (tenant) {
      reset({
        name: tenant.name,
        uid: tenant.uid,
        gid: tenant.gid,
        isolation_level: tenant.isolation_level || 'standard',
      });
    } else {
      reset({
        name: '',
        uid: undefined,
        gid: undefined,
        isolation_level: 'standard',
      });
    }
  }, [tenant, reset]);

  const onSubmit = async (data: FormData) => {
    try {
      if (isEdit && tenant) {
        await updateTenant.mutateAsync({
          tenantId: tenant.id,
          name: data.name,
        });
      } else {
        const createData: CreateTenantRequest = {
          name: data.name,
          uid: data.uid,
          gid: data.gid,
          isolation_level: data.isolation_level,
        };
        await createTenant.mutateAsync(createData);
      }
      onOpenChange(false);
      reset();
    } catch (error) {
      // Error handling is done in the hook
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <form onSubmit={handleSubmit(onSubmit)}>
          <DialogHeader>
            <DialogTitle>{isEdit ? 'Edit Tenant' : 'Create Tenant'}</DialogTitle>
            <DialogDescription>
              {isEdit
                ? 'Update tenant configuration'
                : 'Create a new tenant with isolation settings'}
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="name">
                Name <span className="text-destructive">*</span>
              </Label>
              <Input
                id="name"
                placeholder="tenant-name"
                {...register('name', {
                  required: 'Name is required',
                  pattern: {
                    value: /^[a-z0-9-]+$/,
                    message: 'Name must be lowercase alphanumeric with hyphens',
                  },
                })}
              />
              {errors.name && (
                <p className="text-sm text-destructive">{errors.name.message}</p>
              )}
            </div>

            {!isEdit && (
              <>
                <div className="grid gap-2">
                  <Label htmlFor="isolation_level">Isolation Level</Label>
                  <Select
                    value={isolationLevel}
                    onValueChange={(value) => setValue('isolation_level', value)}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="standard">Standard</SelectItem>
                      <SelectItem value="enhanced">Enhanced</SelectItem>
                      <SelectItem value="strict">Strict</SelectItem>
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    Isolation level determines resource and network separation
                  </p>
                </div>

                <div className="grid gap-2">
                  <Label htmlFor="uid">UID (Optional)</Label>
                  <Input
                    id="uid"
                    type="number"
                    placeholder="1000"
                    {...register('uid', {
                      valueAsNumber: true,
                      min: { value: 1000, message: 'UID must be >= 1000' },
                    })}
                  />
                  {errors.uid && (
                    <p className="text-sm text-destructive">{errors.uid.message}</p>
                  )}
                </div>

                <div className="grid gap-2">
                  <Label htmlFor="gid">GID (Optional)</Label>
                  <Input
                    id="gid"
                    type="number"
                    placeholder="1000"
                    {...register('gid', {
                      valueAsNumber: true,
                      min: { value: 1000, message: 'GID must be >= 1000' },
                    })}
                  />
                  {errors.gid && (
                    <p className="text-sm text-destructive">{errors.gid.message}</p>
                  )}
                </div>
              </>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                onOpenChange(false);
                reset();
              }}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? 'Saving...' : isEdit ? 'Update' : 'Create'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
