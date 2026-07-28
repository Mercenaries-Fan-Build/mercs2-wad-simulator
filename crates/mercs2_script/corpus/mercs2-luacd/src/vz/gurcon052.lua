inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "GurJob008_01_Outpost",
    tCapturePts = {
      "GurJob008_01_CapturePt4"
    },
    sDspShortDesc = "[GurCon052.Objectives.001]",
    sStagingLayer = "Vz_State_GurJob008_01_Staging",
    sStagingTgLayer = "Vz_State_GurCon052_Tg",
    sPristineLayer = "Vz_State_GurJob008_01_Pristine",
    sDefenseLayer = "Vz_State_GurJob008_01_Defenses",
    sCapturedLayer = "Vz_State_GurJob008_01_Captured",
    sCapturedTgLayer = "Vz_State_GurCon052c_Tg",
    sRivalFaction = "Vza",
    tDangerousBldgs = {},
    nStartingHealth = 4,
    nRusherQuota = 1
  }
end
