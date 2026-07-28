inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "PirJob002_03_Outpost",
    tCapturePts = {
      "PirJob002_03_CapturePt2"
    },
    sDspShortDesc = "[PirCon052.Objectives.001]",
    sStagingLayer = "Vz_State_PirJob002_03_Staging",
    sStagingTgLayer = "Vz_State_PirCon052_Tg",
    sPristineLayer = "Vz_State_PirJob002_03_Pristine",
    sDefenseLayer = "Vz_State_PirJob002_03_Defenses",
    sCapturedLayer = "Vz_State_PirJob002_03_Captured",
    sCapturedTgLayer = "Vz_State_PirCon052c_Tg",
    sRivalFaction = "Vza",
    tDangerousBldgs = {},
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end
