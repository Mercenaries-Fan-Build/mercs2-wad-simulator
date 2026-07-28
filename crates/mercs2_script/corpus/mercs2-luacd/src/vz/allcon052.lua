inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "AllJob001_03_Outpost",
    tCapturePts = {
      "AllJob001_03_CapturePt1"
    },
    sStagingLayer = "Vz_State_AllJob001_03_Staging2",
    sStagingTgLayer = "Vz_State_AllCon052_Tg",
    sPristineLayer = "Vz_State_AllJob001_03_Pristine",
    sDefenseLayer = "Vz_State_AllJob001_03_Defenses",
    sCapturedLayer = "Vz_State_AllJob001_03_Captured",
    sCapturedTgLayer = "Vz_State_AllCon052c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All052-03"
  })
end
