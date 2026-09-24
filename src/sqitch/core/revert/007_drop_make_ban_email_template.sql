-- Revert odo:007_drop_make_ban_email_template from pg
--
-- Restores the function exactly as the baseline defined it, so a revert
-- lands on the baseline's state. It is dead code there too.

BEGIN;

CREATE FUNCTION notification.make_ban_email_template(p_title text, p_description text, p_button_text text) RETURNS text
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN '
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="color-scheme" content="light dark">
  <title>' || p_title || '</title>
  <!--[if mso]>
  <noscript>
    <xml>
      <o:OfficeDocumentSettings>
        <o:PixelsPerInch>96</o:PixelsPerInch>
      </o:OfficeDocumentSettings>
    </xml>
  </noscript>
  <![endif]-->
</head>
<body style="margin:0; padding:0; background-color:#f5f5f5; font-family:Segoe UI, Roboto, Helvetica Neue, Helvetica, Arial, sans-serif;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color:#f5f5f5;">
    <tr>
      <td align="center" style="padding:24px 16px;">
        <table role="presentation" width="520" cellspacing="0" cellpadding="0" border="0" style="max-width:520px; background-color:#ffffff;">
          <tr>
            <td style="padding:24px 32px 16px 32px; border-bottom:1px solid #e5e7eb;">
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0">
                <tr>
                  <td>
                    <span style="font-family:Segoe UI, Arial, sans-serif; font-size:20px; font-weight:600; color:#1e3a5f;">Odo Library System</span>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td style="padding:24px 32px;">
              <h1 style="margin:0 0 20px 0; font-family:Segoe UI, Arial, sans-serif; font-size:18px; font-weight:600; color:#111827; line-height:1.4;">
                ' || p_title || '
              </h1>
              <p style="margin:0 0 20px 0; font-family:Segoe UI, Arial, sans-serif; font-size:14px; color:#4b5563; line-height:1.5;">
                ' || p_description || '
              </p>
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color:#f9fafb; margin-bottom:24px;">
                <tr>
                  <td style="padding:16px;">
                    <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0">
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px;">
                          <span style="color:#6b7280;">Location</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right;">
                          {{location_name}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Duration</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{duration}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Start Date</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{date start_date "%b %d, %Y" timezone}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Lift Date</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{#if lift_date}}{{date lift_date "%b %d, %Y" timezone}}{{else}}Indefinite{{/if}}
                        </td>
                      </tr>
                    </table>
                  </td>
                </tr>
              </table>
              <table role="presentation" cellspacing="0" cellpadding="0" border="0" align="center" style="margin:0 auto;">
                <tr>
                  <td align="center" bgcolor="#1e3a5f" style="background-color:#1e3a5f; padding:12px 24px; border-radius:6px; -webkit-border-radius:6px; -moz-border-radius:6px;">
                    <a href="{{incident_url}}" target="_blank" style="font-family:Segoe UI, Arial, sans-serif; font-size:14px; font-weight:600; color:#ffffff; text-decoration:none; display:inline-block; mso-line-height-rule:exactly; line-height:20px;">
                      <span style="color:#ffffff;">' || p_button_text || '</span>
                    </a>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td style="padding:16px 32px 24px 32px; border-top:1px solid #e5e7eb;">
              <p style="margin:0; font-family:Segoe UI, Arial, sans-serif; font-size:12px; color:#9ca3af; line-height:1.5;">
                This is an automated message from Current. Please do not reply.
              </p>
              <p style="margin:8px 0 0 0; font-family:Segoe UI, Arial, sans-serif; font-size:11px; color:#9ca3af;">
                King County Library System
              </p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>';
END;
$$;

COMMIT;
