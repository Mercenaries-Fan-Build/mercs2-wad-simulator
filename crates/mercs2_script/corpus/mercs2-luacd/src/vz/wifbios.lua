function HasBio()
  local nCount = 0
  
  for k, v in pairs(_tActiveBios) do
    nCount = nCount + 1
  end
  return 0 < nCount
end

function SaveSingleton()
  return _tActiveBios
end

function LoadSingleton(tActiveBios)
  if not tActiveBios then
    return
  end
  if not type(tActiveBios) == "table" then
    return
  end
  for k, v in pairs(tActiveBios) do
    if v == true then
      AddDossierEntry(k)
    end
  end
end

function AddDossierEntry(sCurrentBio)
  local tBioData = _tBios[sCurrentBio]
  if tBioData then
    _tActiveBios[sCurrentBio] = true
    Pda.Database:AddDossierEntry({
      sTitle = tBioData.sTitle,
      sText = tBioData.sText,
      sIcon = tBioData.sIcon
    })
  end
end

_tActiveBios = {}
_nNum = 0
_tBios = {
  Default = {
    sTitle = "[PDA.Database.DOSSIERS]",
    sText = "[PDA.Database.Dossiers_Description]",
    sIcon = "icon_people"
  },
  BioChris = {
    sTitle = "[SHELL.SelectCharacter.ChrisJacobs]",
    sText = "[SHELL.SelectCharacter.ChrisJacobsText]",
    sIcon = "icon_people"
  },
  BioJennifer = {
    sTitle = "[SHELL.SelectCharacter.JenniferMui]",
    sText = "[SHELL.SelectCharacter.JenniferMuiText]",
    sIcon = "icon_people"
  },
  BioMattias = {
    sTitle = "[SHELL.SelectCharacter.MattiasNilsson]",
    sText = "[SHELL.SelectCharacter.MattiasNilssonText]",
    sIcon = "icon_people"
  },
  BioAcosta = {
    sTitle = "[SG0.Name]",
    sText = "[SHELL.Bio.Acosta]",
    sIcon = "icon_people"
  },
  BioAllies = {
    sTitle = "[Faction.ALL]",
    sText = "[SHELL.Bio.Allies]",
    sIcon = "icon_people"
  },
  BioBlanco = {
    sTitle = "[SHELL.Bio.Name.Blanco]",
    sText = "[SHELL.Bio.Blanco]",
    sIcon = "icon_people"
  },
  BioCarmona = {
    sTitle = "[human.vz.carmona]",
    sText = "[SHELL.Bio.Carmona]",
    sIcon = "icon_people"
  },
  BioChina = {
    sTitle = "[Faction.CHI]",
    sText = "[SHELL.Bio.China]",
    sIcon = "icon_people"
  },
  BioDevilbwoy = {
    sTitle = "[SP1.Name]",
    sText = "[SHELL.Bio.Devilbwoy]",
    sIcon = "icon_people"
  },
  BioEva = {
    sTitle = "[human.pmc.eva]",
    sText = "[SHELL.Bio.Eva]",
    sIcon = "icon_people"
  },
  BioEwan = {
    sTitle = "[CinematicText.Ewan01]",
    sText = "[SHELL.Bio.Ewan]",
    sIcon = "icon_people"
  },
  BioFiona = {
    sTitle = "[human.pmc.fiona]",
    sText = "[SHELL.Bio.Fiona]",
    sIcon = "icon_people"
  },
  BioJoyce = {
    sTitle = "[SA0.Name]",
    sText = "[SHELL.Bio.Joyce]",
    sIcon = "icon_people"
  },
  BioMisha = {
    sTitle = "[human.pmc.misha]",
    sText = "[SHELL.Bio.Misha]",
    sIcon = "icon_people"
  },
  BioPeng = {
    sTitle = "[SC0.Name]",
    sText = "[SHELL.Bio.Peng]",
    sIcon = "icon_people"
  },
  BioPirates = {
    sTitle = "[Faction.PIR]",
    sText = "[SHELL.Bio.Pirates]",
    sIcon = "icon_people"
  },
  BioPLAV = {
    sTitle = "[Generic.Factions.Gur.Long]",
    sText = "[SHELL.Bio.PLAV]",
    sIcon = "icon_people"
  },
  BioRubin = {
    sTitle = "[SO0.Name]",
    sText = "[SHELL.Bio.Rubin]",
    sIcon = "icon_people"
  },
  BioSolano = {
    sTitle = "[human.vz.solano]",
    sText = "[SHELL.Bio.Solano]",
    sIcon = "icon_people"
  },
  BioUP = {
    sTitle = "[Faction.OIL]",
    sText = "[SHELL.Bio.UP]",
    sIcon = "icon_people"
  }
}
_sCurrentBio = "Default"
