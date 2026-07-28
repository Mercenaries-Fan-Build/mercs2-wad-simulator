inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "ChiJob001_04_Outpost",
    tCapturePts = {
      "ChiJob001_04_CapturePt1"
    },
    sStagingLayer = "Vz_State_ChiJob001_04_Staging",
    sStagingTgLayer = "Vz_State_ChiCon053_Tg",
    sPristineLayer = "Vz_State_ChiJob001_04_Pristine",
    sDefenseLayer = "Vz_State_ChiJob001_04_Defenses",
    sCapturedLayer = "Vz_State_ChiJob001_04_Captured",
    sCapturedTgLayer = "Vz_State_ChiCon053c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi053-01",
    {
      mattias = "Mattias-In-Mission-Contract-Chi053-04",
      jennifer = "Jennifer-In-Mission-Contract-Chi053-02",
      chris = "Chris-In-Mission-Contract-Chi053-03"
    }
  })
end
