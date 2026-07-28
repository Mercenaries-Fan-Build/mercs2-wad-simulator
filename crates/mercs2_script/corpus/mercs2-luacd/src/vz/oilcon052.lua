inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "OilJob005_Outpost",
    tCapturePts = {
      "OilJob005_CapturePt2"
    },
    sDspShortDesc = "[OilCon052.Objectives.001]",
    sStagingLayer = "Vz_State_OilJob005_Staging",
    sStagingTgLayer = "Vz_State_OilCon052_Tg",
    sPristineLayer = "Vz_State_OilJob005_Pristine",
    sDefenseLayer = "Vz_State_OilJob005_Defenses",
    sCapturedLayer = "Vz_State_OilJob005_Captured",
    sCapturedTgLayer = "Vz_State_OilCon052c_Tg",
    tDangerousBldgs = {},
    sRivalFaction = "Vza",
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end
