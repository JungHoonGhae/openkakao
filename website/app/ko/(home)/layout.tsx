import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { baseOptions } from '@/lib/layout.shared';

export default function Layout({ children }: LayoutProps<'/ko'>) {
  return <HomeLayout {...baseOptions('ko', 'home')}>{children}</HomeLayout>;
}
