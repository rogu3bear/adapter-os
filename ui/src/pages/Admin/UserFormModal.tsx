import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { FormModalWithHookForm } from '@/components/shared/Modal';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useCreateUser, useUpdateUser, useTenants } from '@/hooks/admin/useAdmin';
import type { User, UserRole, RegisterUserRequest, UpdateUserRequest } from '@/api/types';
import { UserFormSchema, type UserFormData } from '@/schemas/admin.schema';

interface UserFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  user?: User;
}

const ROLE_OPTIONS: { value: UserRole; label: string; description: string }[] = [
  { value: 'admin', label: 'Admin', description: 'Full system access' },
  { value: 'operator', label: 'Operator', description: 'Manage deployments and operations' },
  { value: 'sre', label: 'SRE', description: 'System health and infrastructure' },
  { value: 'compliance', label: 'Compliance', description: 'Audit access' },
  { value: 'auditor', label: 'Auditor', description: 'Read-only audit logs' },
  { value: 'viewer', label: 'Viewer', description: 'Read-only access' },
];

export function UserFormModal({ open, onOpenChange, user }: UserFormModalProps) {
  const isEdit = !!user;
  const createUser = useCreateUser();
  const updateUser = useUpdateUser();
  const { data: tenants } = useTenants();

  const form = useForm<UserFormData>({
    resolver: zodResolver(UserFormSchema),
    defaultValues: {
      email: user?.email || '',
      display_name: user?.display_name || '',
      role: user?.role || 'viewer',
      tenant_id: user?.tenant_id || '',
    },
  });

  const { register, formState: { errors }, reset, setValue, watch } = form;
  const selectedRole = watch('role');
  const selectedTenantId = watch('tenant_id');

  useEffect(() => {
    if (user) {
      reset({
        email: user.email,
        display_name: user.display_name || '',
        role: user.role || 'viewer',
        tenant_id: user.tenant_id || '',
      });
    } else {
      reset({
        email: '',
        password: '',
        display_name: '',
        role: 'viewer',
        tenant_id: '',
      });
    }
  }, [user, reset]);

  const onSubmit = async (data: UserFormData) => {
    if (isEdit && user) {
      const userId = user.user_id || user.id;
      if (!userId) {
        throw new Error('User ID is required for update');
      }
      const updateData: UpdateUserRequest = {
        display_name: data.display_name,
        role: data.role,
      };
      await updateUser.mutateAsync({
        userId,
        data: updateData,
      });
    } else {
      if (!data.password) {
        throw new Error('Password is required for new users');
      }
      const createData: RegisterUserRequest = {
        email: data.email,
        password: data.password,
        display_name: data.display_name,
        role: data.role,
        tenant_id: data.tenant_id || undefined,
      };
      await createUser.mutateAsync(createData);
    }
  };

  return (
    <FormModalWithHookForm
      open={open}
      onOpenChange={onOpenChange}
      title={isEdit ? 'Edit User' : 'Create User'}
      description={
        isEdit
          ? 'Update user details and role assignment'
          : 'Create a new user account with role assignment'
      }
      form={form}
      onSubmit={onSubmit}
      submitText={isEdit ? 'Update' : 'Create'}
      size="lg"
    >
      <div className="grid gap-4">
            {/* Email */}
            <div className="grid gap-2">
              <Label htmlFor="email">
                Email <span className="text-destructive">*</span>
              </Label>
              <Input
                id="email"
                type="email"
                placeholder="user@example.com"
                disabled={isEdit}
                {...register('email')}
              />
              {errors.email && (
                <p className="text-sm text-destructive">{errors.email.message}</p>
              )}
            </div>

            {/* Password (only for create) */}
            {!isEdit && (
              <div className="grid gap-2">
                <Label htmlFor="password">
                  Password <span className="text-destructive">*</span>
                </Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter a strong password"
                  {...register('password')}
                />
                {errors.password && (
                  <p className="text-sm text-destructive">{errors.password.message}</p>
                )}
                <p className="text-xs text-muted-foreground">
                  Minimum 8 characters. Use a mix of letters, numbers, and symbols.
                </p>
              </div>
            )}

            {/* Display Name */}
            <div className="grid gap-2">
              <Label htmlFor="display_name">Display Name</Label>
              <Input
                id="display_name"
                placeholder="John Doe"
                {...register('display_name')}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Used for display purposes throughout the UI.
              </p>
            </div>

            {/* Role */}
            <div className="grid gap-2">
              <Label htmlFor="role">
                Role <span className="text-destructive">*</span>
              </Label>
              <Select
                value={selectedRole}
                onValueChange={(value) => setValue('role', value as UserRole)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select a role" />
                </SelectTrigger>
                <SelectContent>
                  {ROLE_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      <div className="flex flex-col">
                        <span className="font-medium">{option.label}</span>
                        <span className="text-xs text-muted-foreground">
                          {option.description}
                        </span>
                      </div>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Determines what actions the user can perform in the system.
              </p>
            </div>

            {/* Tenant (only for create) */}
            {!isEdit && tenants && tenants.length > 0 && (
              <div className="grid gap-2">
                <Label htmlFor="tenant_id">Workspace</Label>
                <Select
                  value={selectedTenantId || '__none__'}
                  onValueChange={(value) => setValue('tenant_id', value === '__none__' ? undefined : value)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select a workspace (optional)" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">
                      <span className="text-muted-foreground">Global (no workspace)</span>
                    </SelectItem>
                    {tenants.map((tenant) => (
                      <SelectItem key={tenant.id} value={tenant.id}>
                        {tenant.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  Optionally assign the user to a specific organization for scoped access.
                </p>
              </div>
            )}
          </div>
    </FormModalWithHookForm>
  );
}
