inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "PirJob002_02_Outpost",
    tCapturePts = {
      "PirJob002_02_CapturePt1"
    },
    sDspShortDesc = "[PirCon051.Objectives.001]",
    sStagingLayer = "Vz_State_PirJob002_02_Staging",
    sStagingTgLayer = "Vz_State_PirCon051_Tg",
    sPristineLayer = "Vz_State_PirJob002_02_Pristine",
    sDefenseLayer = "Vz_State_PirJob002_02_Defenses",
    sCapturedLayer = "Vz_State_PirJob002_02_Captured",
    sCapturedTgLayer = "Vz_State_PirCon051c_Tg",
    sRivalFaction = "Vza",
    tDangerousBldgs = {},
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end
