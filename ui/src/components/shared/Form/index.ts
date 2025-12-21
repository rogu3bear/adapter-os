/**
 * Form Components
 *
 * A set of reusable form components that integrate with react-hook-form and Zod validation.
 *
 * @example Basic form usage with specialized fields
 * ```tsx
 * import { useForm } from "react-hook-form";
 * import { zodResolver } from "@hookform/resolvers/zod";
 * import { z } from "zod";
 * import {
 *   FormField,
 *   FormSection,
 *   FormActions,
 *   SelectField,
 *   TextareaField,
 *   NumberField,
 *   SwitchField,
 *   DateField,
 *   FileField,
 * } from "@/components/shared/Form";
 *
 * const schema = z.object({
 *   email: z.string().email("Invalid email address"),
 *   name: z.string().min(2, "Name must be at least 2 characters"),
 *   bio: z.string().optional(),
 *   role: z.string(),
 *   age: z.number().min(18),
 *   active: z.boolean(),
 *   startDate: z.date(),
 *   avatar: z.any(),
 * });
 *
 * type FormData = z.infer<typeof schema>;
 *
 * function MyForm() {
 *   const form = useForm<FormData>({
 *     resolver: zodResolver(schema),
 *     defaultValues: { email: "", name: "", bio: "", role: "", age: 18, active: true },
 *   });
 *
 *   const onSubmit = async (data: FormData) => {
 *     // Handle submission
 *   };
 *
 *   return (
 *     <form onSubmit={form.handleSubmit(onSubmit)}>
 *       <FormSection title="Contact Information">
 *         <FormField form={form} name="email" label="Email" type="email" required />
 *         <FormField form={form} name="name" label="Full Name" required />
 *       </FormSection>
 *
 *       <FormSection title="Profile Details" collapsible>
 *         <TextareaField
 *           control={form.control}
 *           name="bio"
 *           label="Biography"
 *           maxLength={500}
 *           showCharacterCount
 *         />
 *         <SelectField
 *           control={form.control}
 *           name="role"
 *           label="Role"
 *           options={[
 *             { value: "admin", label: "Admin" },
 *             { value: "user", label: "User" },
 *           ]}
 *         />
 *         <NumberField
 *           control={form.control}
 *           name="age"
 *           label="Age"
 *           min={18}
 *           max={120}
 *           showButtons
 *         />
 *         <SwitchField control={form.control} name="active" label="Active" />
 *         <DateField control={form.control} name="startDate" label="Start Date" />
 *         <FileField control={form.control} name="avatar" label="Avatar" accept="image/*" />
 *       </FormSection>
 *
 *       <FormActions
 *         isSubmitting={form.formState.isSubmitting}
 *         isDirty={form.formState.isDirty}
 *         isValid={form.formState.isValid}
 *         onReset={() => form.reset()}
 *         showReset
 *       />
 *     </form>
 *   );
 * }
 * ```
 */

// Generic form field component (legacy, supports multiple types via `type` prop)
export { FormField } from "./FormField";

// Re-export types from types.ts (legacy types for FormField component)
export type {
  ValidationState,
  FormFieldBaseProps,
  TextFieldProps,
  TextareaFieldProps,
  NumberFieldProps,
  SelectFieldProps,
  SelectOption,
  CheckboxFieldProps,
  FieldError,
  FormFieldRenderProps,
  FormFieldProps,
  FormSectionProps,
  FormActionsProps,
  FormSchema,
  FormConfig,
  FieldConfig,
  FormFieldsConfig,
} from "./types";

// Re-export base form components from ui/form for convenience
export {
  Form,
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
  useFormField,
} from "@/components/ui/form";
