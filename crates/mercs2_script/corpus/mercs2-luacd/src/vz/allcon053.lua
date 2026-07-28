inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "AllJob001_04_Outpost",
    tCapturePts = {
      "AllJob001_04_CapturePt1"
    },
    sStagingLayer = "Vz_State_AllJob001_04_Staging",
    sStagingTgLayer = "Vz_State_AllCon053_Tg",
    sPristineLayer = "Vz_State_AllJob001_04_Pristine",
    sDefenseLayer = "Vz_State_AllJob001_04_Defenses",
    sCapturedLayer = "Vz_State_AllJob001_04_Captured",
    sCapturedTgLayer = "Vz_State_AllCon053c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All053-01"
  })
end
