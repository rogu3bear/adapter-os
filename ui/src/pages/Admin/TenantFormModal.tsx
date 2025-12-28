import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { FormModalWithHookForm } from '@/components/shared/Modal';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useCreateTenant, useUpdateTenant } from '@/hooks/admin/useAdmin';
import type { Tenant, CreateTenantRequest } from '@/api/types';
import { TenantFormSchema, type TenantFormData } from '@/schemas/admin.schema';

interface TenantFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant?: Tenant;
}

export function TenantFormModal({ open, onOpenChange, tenant }: TenantFormModalProps) {
  const isEdit = !!tenant;
  const createTenant = useCreateTenant();
  const updateTenant = useUpdateTenant();

  const form = useForm<TenantFormData>({
    resolver: zodResolver(TenantFormSchema),
    defaultValues: {
      name: tenant?.name || '',
      description: tenant?.description || '',
      uid: tenant?.uid,
      gid: tenant?.gid,
      isolation_level: (tenant?.isolation_level as 'standard' | 'enhanced' | 'strict') || undefined,
    },
  });

  const { register, formState: { errors }, reset, setValue, watch } = form;
  const isolationLevel = watch('isolation_level');

  useEffect(() => {
    if (tenant) {
      reset({
        name: tenant.name,
        description: tenant.description || '',
        uid: tenant.uid,
        gid: tenant.gid,
        isolation_level: (tenant.isolation_level as 'standard' | 'enhanced' | 'strict') || undefined,
      });
    } else {
      reset({
        name: '',
        description: '',
        uid: undefined,
        gid: undefined,
        isolation_level: undefined,
      });
    }
  }, [tenant, reset]);

  const onSubmit = async (data: TenantFormData) => {
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
  };

  return (
    <FormModalWithHookForm
      open={open}
      onOpenChange={onOpenChange}
      title={isEdit ? 'Edit Workspace' : 'Create Workspace'}
      description={
        isEdit
          ? 'Update organization configuration'
          : 'Create a new organization with isolation settings'
      }
      form={form}
      onSubmit={onSubmit}
      submitText={isEdit ? 'Update' : 'Create'}
      size="lg"
    >
      <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="name">
                Name <span className="text-destructive">*</span>
              </Label>
              <Input
                id="name"
                placeholder="acme-corp"
                {...register('name')}
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
                    onValueChange={(value) => setValue('isolation_level', value as 'standard' | 'enhanced' | 'strict')}
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
                    {...register('uid', { valueAsNumber: true })}
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
                    {...register('gid', { valueAsNumber: true })}
                  />
                  {errors.gid && (
                    <p className="text-sm text-destructive">{errors.gid.message}</p>
                  )}
                </div>
              </>
            )}
          </div>
    </FormModalWithHookForm>
  );
}
