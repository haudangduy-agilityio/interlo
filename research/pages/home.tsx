import Head from "next/head";
import React from "react";
import SignUpScreen from "./_components/SignUpScreen";

export default function HomePage() {
  // const path = usePathname()
  // productionLogger.debug(path)
  return (
    <React.Fragment>
      <Head>
        <title>MeetingCopilot</title>
      </Head>
      <SignUpScreen />
    </React.Fragment>
  );
}
