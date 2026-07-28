inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "ChiJob001_01_Outpost",
    tCapturePts = {
      "ChiJob001_01_CapturePt3"
    },
    sStagingLayer = "Vz_State_ChiJob001_01_Staging",
    sStagingTgLayer = "Vz_State_ChiCon050_Tg",
    sPristineLayer = "Vz_State_ChiJob001_01_Pristine",
    sDefenseLayer = "Vz_State_ChiJob001_01_Defenses",
    sCapturedLayer = "Vz_State_ChiJob001_01_Captured",
    sCapturedTgLayer = "Vz_State_ChiCon050c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi050-01"
  })
end
