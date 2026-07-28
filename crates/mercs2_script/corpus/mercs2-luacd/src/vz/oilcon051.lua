inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "OilJob002_Outpost",
    tCapturePts = {
      "OilJob002_CapturePt5"
    },
    sDspShortDesc = "[OilCon051.Objectives.001]",
    sStagingLayer = "Vz_State_OilJob002_Staging",
    sStagingTgLayer = "Vz_State_OilCon051_Tg",
    sPristineLayer = "Vz_State_OilJob002_Pristine",
    sDefenseLayer = "Vz_State_OilJob002_Defenses",
    sCapturedLayer = "Vz_State_OilJob002_Captured",
    sCapturedTgLayer = "Vz_State_OilCon051c_Tg",
    sRivalFaction = "Vza",
    tDangerousBldgs = {},
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end
