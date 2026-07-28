inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "GurJob003_Outpost",
    tCapturePts = {
      "GurJob003_CapturePt1"
    },
    sDspShortDesc = "[GurCon050.Objectives.001]",
    sStagingLayer = "Vz_State_GurJob003_Staging",
    sStagingTgLayer = "Vz_State_GurCon050_Tg",
    sPristineLayer = "Vz_State_GurJob003_Pristine",
    sDefenseLayer = "Vz_State_GurJob003_Defenses",
    sCapturedLayer = "Vz_State_GurJob003_Captured",
    sCapturedTgLayer = "Vz_State_GurCon050c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end
