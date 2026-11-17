import { useRouter } from "next/router";
import { useEffect } from "react";
import { useAppDispatch, useAppSelector } from "../../redux/hooks";
import { setCurrentLanguage, SUPPORTED_LOCALIZATIONS, type Localization } from "../../redux/slices/languageSlice";
import { DesktopShell } from "../../components/desktop-home/DesktopShell";

const Dashboard = () => {
  const { user } = useAppSelector((state) => state.user);
  const dispatch = useAppDispatch();
  const router = useRouter();

  useEffect(() => {
    if (user && !user?.onboarded) {
      router.replace("/home/");
    }
  }, [user, router]);

  useEffect(() => {
    const languagePreference = user?.languagePreference;
    const nextLanguage: Localization =
      languagePreference && SUPPORTED_LOCALIZATIONS.includes(languagePreference)
        ? languagePreference
        : "en";

    dispatch(setCurrentLanguage(nextLanguage));
  }, [user?.languagePreference, dispatch]);
  return <DesktopShell />;
};

export default Dashboard;
