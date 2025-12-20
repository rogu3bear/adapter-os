import PageWrapper from '@/layout/PageWrapper';
import { ComplianceTab } from './ComplianceTab';

export default function CompliancePage() {
  return (
    <PageWrapper
      pageKey="compliance"
      title="Compliance"
      description="Compliance dashboard and reporting"
    >
      <ComplianceTab />
    </PageWrapper>
  );
}
